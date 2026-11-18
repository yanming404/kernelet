// SPDX-License-Identifier: GPL-2.0
#include "shim.h"
#include "../../linux/endovisor/typedio_host.h"
extern void native_vsock_config(struct klet_tio_descriptor *,u64,u64);
extern void native_vsock_guest_roundtrip(void);
static struct klet_carrier carriers[2];
static _Thread_local unsigned int active;
void native_select_guest(u32 index){assert(index<2);active=index;}
s64 native_guest_service(struct klet_tio_call *call){return klet_tio_service(&carriers[active],call);}
int native_drain_irq_sources(void){
    unsigned int bits=atomic_exchange(&carriers[active].pending,0);assert(bits&KLET_VIRQ_LINES);
    struct klet_tio_call call={.operation=KLET_TIO_IRQ_POLL};int count=0;
    while(!klet_tio_service(&carriers[active],&call)){assert(call.record.sequence==7&&call.record.generation==1);assert(++count<64);}
    return count;
}
int main(void){
 struct cgroup owner={1};native_task.group=&owner;
 struct klet_inst instances[2]={0};
 assert(!klet_tio_host_init());assert(!klet_tio_vsock_register());
 for(unsigned int i=0;i<2;i++){
  struct klet_inst *inst=&instances[i];pthread_mutex_init(&inst->mutex,NULL);spin_lock_init(&inst->lock);inst->cgrp=&owner;inst->nr_vcpus=1;refcount_set(&inst->refs,1);carriers[i].inst=inst;inst->vcpus[0]=&carriers[i];
  struct klet_tio_descriptor descriptor;native_vsock_config(&descriptor,i?7:42,i?42:7);
  int fd=klet_tio_create_device(&descriptor);assert(fd>=3);struct klet_device attach={.type=KLET_DEVICE_TYPEDIO,.fd=fd};assert(klet_tio_attach(inst,&attach)==0);native_close(fd);
 }
 native_vsock_guest_roundtrip();
 for(unsigned int i=0;i<2;i++){instances[i].dying=true;klet_tio_detach_instance(&instances[i]);klet_tio_resources_evicted(&instances[i]);}
 klet_tio_vsock_unregister();klet_tio_host_exit();assert(atomic_load(&native_module.references)==0);
 puts("PASS native mock: production guest Queue/Pipe/VmIO -> carrier service -> host framework -> safe Vsock backend -> peer guest; bidirectional echo/peek/EOF/cancel/retirement");return 0;
}
