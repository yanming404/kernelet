// SPDX-License-Identifier: MPL-2.0

//! Trusted recursive admission and padding-free transfer generation.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, parse_quote, Data, DeriveInput, Fields, Index, Type};

#[proc_macro_derive(Message)]
pub fn message(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let packed = input.attrs.iter().any(|attribute| {
        if !attribute.path().is_ident("repr") {
            return false;
        }
        let mut found = false;
        let _ = attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("packed") {
                found = true;
            }
            if meta.input.peek(syn::token::Paren) {
                let nested;
                syn::parenthesized!(nested in meta.input);
                let _ = nested.parse::<proc_macro2::TokenStream>()?;
            }
            Ok(())
        });
        found
    });
    let variants: Vec<(&Fields, Option<&syn::Ident>)> = match &input.data {
        Data::Struct(s) => vec![(&s.fields, None)],
        Data::Enum(e) => e
            .variants
            .iter()
            .map(|v| (&v.fields, Some(&v.ident)))
            .collect(),
        Data::Union(u) => {
            return Err(syn::Error::new_spanned(
                u.union_token,
                "unions are not exchangeable TypedIO messages",
            ))
        }
    };
    let types: Vec<&Type> = variants
        .iter()
        .flat_map(|(f, _)| f.iter().map(|x| &x.ty))
        .collect();
    let mut admitted = input.generics.clone();
    admitted
        .make_where_clause()
        .predicates
        .push(parse_quote!(Self:'static));
    for ty in &types {
        admitted
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: ::typedio::Message));
    }
    let (impl_g, type_g, where_g) = admitted.split_for_impl();
    let (original_impl_g, original_type_g, original_where_g) = input.generics.split_for_impl();
    // Identifiers, field names, variant boundaries and explicit discriminants
    // are canonical source-level schema data; native repr/padding are unused.
    let shape = match &input.data {
        Data::Struct(s) => format!("struct {} {}", name, fields_shape(&s.fields)),
        Data::Enum(e) => format!(
            "enum {} {}",
            name,
            e.variants
                .iter()
                .map(|v| {
                    let id = &v.ident;
                    let discr = &v.discriminant;
                    let discr = discr
                        .as_ref()
                        .map(|(_, e)| quote!(#e).to_string())
                        .unwrap_or_default();
                    format!("{}={}{}", id, discr, fields_shape(&v.fields))
                })
                .collect::<Vec<_>>()
                .join(";")
        ),
        Data::Union(_) => unreachable!(),
    };
    let sizes: Vec<_> = variants
        .iter()
        .map(|(f, _)| {
            let types = f.iter().map(|x| &x.ty);
            quote!(0usize #(+ <#types as ::typedio::message::Wire>::MAX_SIZE)*)
        })
        .collect();
    let is_enum = matches!(input.data, Data::Enum(_));
    let (encoding, decoding, max_size) = if !is_enum {
        let fields = variants[0].0;
        let accesses = fields.iter().enumerate().map(|(i, f)| match &f.ident {
            Some(id) => quote!(#id),
            None => {
                let i = Index::from(i);
                quote!(#i)
            }
        });
        let decode = construct(quote!(Self), fields);
        let size = &sizes[0];
        let encoding = if packed {
            quote!(#(
                // SAFETY: the shared self borrow keeps this admitted field
                // valid and stable; the helper never drops its aligned snapshot.
                unsafe { ::typedio::message::__encode_unaligned(
                    ::core::ptr::addr_of!(self.#accesses), out
                )?; }
            )* Ok(()))
        } else {
            quote!(#(::typedio::message::Wire::encode(&self.#accesses,out)?;)* Ok(()))
        };
        (encoding, quote!(Ok(#decode)), quote!(#size))
    } else {
        let mut enc = Vec::new();
        let mut dec = Vec::new();
        for (index, (fields, variant)) in variants.iter().enumerate() {
            let variant = variant.unwrap();
            let tag = index as u32;
            let bindings: Vec<_> = (0..fields.len())
                .map(|i| format_ident!("__field_{i}"))
                .collect();
            let pattern = match fields {
                Fields::Named(f) => {
                    let names = f.named.iter().map(|f| &f.ident);
                    quote!(Self::#variant{#(#names:#bindings),*})
                }
                Fields::Unnamed(_) => quote!(Self::#variant(#(#bindings),*)),
                Fields::Unit => quote!(Self::#variant),
            };
            enc.push(quote!(#pattern=>{::typedio::message::Wire::encode(&#tag,out)?;#(::typedio::message::Wire::encode(#bindings,out)?;)* Ok(())}));
            let value = construct(quote!(Self::#variant), fields);
            dec.push(quote!(#tag=>Ok(#value)));
        }
        (
            quote!(match self {#(#enc),*}),
            quote!(match <u32 as ::typedio::message::Wire>::decode(input)?{#(#dec,)* _=>Err(::typedio::message::TransferError::InvalidValue)}),
            quote!({let sizes=[0usize,#(#sizes),*];let mut max=0;let mut i=0;while i<sizes.len(){if sizes[i]>max{max=sizes[i];}i+=1;}4usize+max}),
        )
    };
    let committed = if !is_enum {
        let members = variants[0]
            .0
            .iter()
            .enumerate()
            .map(|(i, f)| match &f.ident {
                Some(id) => quote!(#id),
                None => {
                    let i = Index::from(i);
                    quote!(#i)
                }
            });
        quote!(#(unsafe { ::typedio::message::__committed_unaligned(
            ::core::ptr::addr_of_mut!(self.#members)); })*)
    } else {
        let arms = variants.iter().map(|(fields, variant)| {
            let variant = variant.unwrap();
            let bindings: Vec<_> = (0..fields.len())
                .map(|i| format_ident!("__field_{i}"))
                .collect();
            let pattern = match fields {
                Fields::Named(f) => {
                    let names = f.named.iter().map(|f| &f.ident);
                    quote!(Self::#variant{#(#names:#bindings),*})
                }
                Fields::Unnamed(_) => quote!(Self::#variant(#(#bindings),*)),
                Fields::Unit => quote!(Self::#variant),
            };
            quote!(#pattern=>{#(::typedio::Message::__committed(#bindings);)*})
        });
        quote!(match self {#(#arms),*})
    };
    Ok(quote! {
        // The overlapping implementation makes adding custom Drop a hard
        // compile error, including when it is defined after this derive.
        const _: () = {
            trait __TypedIoNoCustomDrop {}
            #[allow(drop_bounds)]
            impl<T: ::core::ops::Drop> __TypedIoNoCustomDrop for T {}
            impl #original_impl_g __TypedIoNoCustomDrop for #name #original_type_g #original_where_g {}
        };
        // SAFETY: all fields have recursive Message bounds; generated field
        // transfer never observes object padding or sender allocator state.
        unsafe impl #impl_g ::typedio::Message for #name #type_g #where_g {
            const SCHEMA: ::typedio::SchemaDigest=::typedio::SchemaDigest::of(#shape,&[#(<#types as ::typedio::Message>::SCHEMA),*],&[]);
            const __MAX_SIZE:usize=#max_size;
            fn __committed(&mut self){#committed}
            fn __encode(&self,out:&mut ::typedio::message::Encoder<'_>)->Result<(),::typedio::message::TransferError>{#encoding}
            fn __decode(input:&mut ::typedio::message::Decoder<'_>)->Result<Self,::typedio::message::TransferError>{#decoding}
        }
    })
}
fn fields_shape(fields: &Fields) -> String {
    let names = fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            f.ident
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_else(|| i.to_string())
        })
        .collect::<Vec<_>>()
        .join(",");
    match fields {
        Fields::Named(_) => format!("{{{names}}}"),
        Fields::Unnamed(_) => format!("({names})"),
        Fields::Unit => "unit".into(),
    }
}
fn construct(prefix: proc_macro2::TokenStream, fields: &Fields) -> proc_macro2::TokenStream {
    let values = fields.iter().map(|f| {
        let ty = &f.ty;
        quote!(<#ty as ::typedio::message::Wire>::decode(input)?)
    });
    match fields {
        Fields::Named(f) => {
            let names = f.named.iter().map(|f| &f.ident);
            quote!(#prefix{#(#names:#values),*})
        }
        Fields::Unnamed(_) => quote!(#prefix(#(#values),*)),
        Fields::Unit => prefix,
    }
}
