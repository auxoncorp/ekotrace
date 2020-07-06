//! A wire protocol for representing Modality probe causal snaphots

use crate::{wire::le_bytes, ProbeId};
use core::mem::size_of;
use static_assertions::const_assert_eq;

/// Everything that can go wrong when attempting to interpret a causal snaphot
/// from the wire representation
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum CausalSnapshotWireError {
    /// There weren't enough bytes for a full causal snapshot
    MissingBytes,
    /// The probe id didn't follow the rules for being
    /// a valid Modality probe-specifying ProbeId
    InvalidProbeId(u32),
}

/// A read/write wrapper around a causal snaphot buffer
#[derive(Debug, Clone)]
pub struct CausalSnapshot<T: AsRef<[u8]>> {
    buffer: T,
}

const_assert_eq!(size_of::<crate::CausalSnapshot>(), field::REST.start);

mod field {
    type Field = ::core::ops::Range<usize>;
    type Rest = ::core::ops::RangeFrom<usize>;

    /// LogicalClock.id
    pub const PROBE_ID: Field = 0..4;
    /// LogicalClock.count
    pub const COUNT: Field = 4..8;
    /// Reserved field
    pub const RESERVED_0: Field = 8..10;
    /// Reserved field
    pub const RESERVED_1: Field = 10..12;
    /// Remaining bytes
    pub const REST: Rest = 12..;
}

impl<T: AsRef<[u8]>> CausalSnapshot<T> {
    /// Construct a causal snaphot from a byte buffer
    pub fn new_unchecked(buffer: T) -> CausalSnapshot<T> {
        CausalSnapshot { buffer }
    }

    /// Construct a causal snaphot from a byte buffer, with checks.
    ///
    /// A combination of:
    /// * [new_unchecked](struct.CausalSnapshot.html#method.new_unchecked)
    /// * [check_len](struct.CausalSnapshot.html#method.check_len)
    pub fn new(buffer: T) -> Result<CausalSnapshot<T>, CausalSnapshotWireError> {
        let r = Self::new_unchecked(buffer);
        r.check_len()?;
        Ok(r)
    }

    /// Ensure that no accessor method will panic if called.
    ///
    /// Returns `Err(CausalSnapshotWireError::MissingBytes)` if the buffer
    /// is too short.
    pub fn check_len(&self) -> Result<(), CausalSnapshotWireError> {
        let len = self.buffer.as_ref().len();
        if len < field::REST.start {
            Err(CausalSnapshotWireError::MissingBytes)
        } else {
            Ok(())
        }
    }

    /// Consumes the causal snaphot, returning the underlying buffer
    pub fn into_inner(self) -> T {
        self.buffer
    }

    /// Return the length of a buffer required to hold a causal snaphot
    pub fn buffer_len() -> usize {
        field::REST.start
    }

    /// Return the `probe_id` field
    #[inline]
    pub fn probe_id(&self) -> Result<ProbeId, CausalSnapshotWireError> {
        let data = self.buffer.as_ref();
        let raw_probe_id = le_bytes::read_u32(&data[field::PROBE_ID]);
        match ProbeId::new(raw_probe_id) {
            Some(id) => Ok(id),
            None => Err(CausalSnapshotWireError::InvalidProbeId(raw_probe_id)),
        }
    }

    /// Return the `count` field
    #[inline]
    pub fn count(&self) -> u32 {
        let data = self.buffer.as_ref();
        le_bytes::read_u32(&data[field::COUNT])
    }

    /// Return the `reserved_0` field
    #[inline]
    pub fn reserved_0(&self) -> u16 {
        let data = self.buffer.as_ref();
        le_bytes::read_u16(&data[field::RESERVED_0])
    }

    /// Return the `reserved_1` field
    #[inline]
    pub fn reserved_1(&self) -> u16 {
        let data = self.buffer.as_ref();
        le_bytes::read_u16(&data[field::RESERVED_1])
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> CausalSnapshot<T> {
    /// Set the `probe_id` field
    #[inline]
    pub fn set_probe_id(&mut self, value: ProbeId) {
        let data = self.buffer.as_mut();
        le_bytes::write_u32(&mut data[field::PROBE_ID], value.get_raw());
    }

    /// Set the `count` field
    #[inline]
    pub fn set_count(&mut self, value: u32) {
        let data = self.buffer.as_mut();
        le_bytes::write_u32(&mut data[field::COUNT], value);
    }

    /// Set the `reserved_0` field
    #[inline]
    pub fn set_reserved_0(&mut self, value: u16) {
        let data = self.buffer.as_mut();
        le_bytes::write_u16(&mut data[field::RESERVED_0], value);
    }

    /// Set the `reserved_1` field
    #[inline]
    pub fn set_reserved_1(&mut self, value: u16) {
        let data = self.buffer.as_mut();
        le_bytes::write_u16(&mut data[field::RESERVED_1], value);
    }
}

impl<T: AsRef<[u8]>> AsRef<[u8]> for CausalSnapshot<T> {
    fn as_ref(&self) -> &[u8] {
        self.buffer.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rustfmt::skip]
    static SNAPSHOT_BYTES: [u8; 12] = [
        // probe_id: 1
        0x01, 0x00, 0x00, 0x00,
        // count: 2
        0x02, 0x00, 0x00, 0x00,
        // reserved_0: 3
        0x03, 0x00,
        // reserved_1: 4
        0x04, 0x00,
    ];

    #[test]
    fn buffer_len() {
        assert_eq!(CausalSnapshot::<&[u8]>::buffer_len(), 12);
    }

    #[test]
    fn construct() {
        let mut bytes = [0xFF; 12];
        let mut s = CausalSnapshot::new_unchecked(&mut bytes[..]);
        assert_eq!(s.check_len(), Ok(()));
        s.set_probe_id(ProbeId::new(1).unwrap());
        s.set_count(2);
        s.set_reserved_0(3);
        s.set_reserved_1(4);
        assert_eq!(&s.into_inner()[..], &SNAPSHOT_BYTES[..]);
    }

    #[test]
    fn deconstruct() {
        let s = CausalSnapshot::new(&SNAPSHOT_BYTES[..]).unwrap();
        assert_eq!(s.probe_id().unwrap().get_raw(), 1);
        assert_eq!(s.count(), 2);
        assert_eq!(s.reserved_0(), 3);
        assert_eq!(s.reserved_1(), 4);
    }

    #[test]
    fn missing_bytes() {
        let bytes = [0xFF; 12 - 1];
        assert_eq!(bytes.len(), CausalSnapshot::<&[u8]>::buffer_len() - 1);
        let s = CausalSnapshot::new(&bytes[..]);
        assert_eq!(s.unwrap_err(), CausalSnapshotWireError::MissingBytes);
    }
}
