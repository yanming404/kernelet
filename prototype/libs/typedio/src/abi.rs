// SPDX-License-Identifier: MPL-2.0

//! Scalar-only records shared by the C wrapper and Rust metadata engine.
//! These records are identities and bounds, never memory-access authority.

/// A host-private snapshot of one authorized reservation.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReservationRecord {
    /// A non-reused token within this pipe's lifetime.
    pub token: u64,
    /// The byte offset from the pipe's retained backing.
    pub offset: u64,
    /// The contiguous byte bound of this reservation.
    pub length: u64,
    /// The prefix already measured by byte-transfer helpers.
    pub progress: u64,
}
