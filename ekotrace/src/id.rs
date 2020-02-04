//! Identifiers critical to the Ekotrace system
use crate::{InvalidEventId, InvalidTracerId};
use core::convert::{TryFrom, TryInto};
use core::num::NonZeroU32;

/// Ought to uniquely identify a location for where events occur within a system under test.
///
/// Typically represents a single thread.
///
/// Must be backed by a value greater than 0 and less than 0b1000_0000_0000_0000_0000_0000_0000_0000
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct TracerId(NonZeroU32);

impl TracerId {
    /// The largest permissible backing id value
    pub const MAX_ID: u32 = 0b0111_1111_1111_1111_1111_1111_1111_1111;

    /// raw_id must be greater than 0 and less than 0b1000_0000_0000_0000_0000_0000_0000_0000
    #[inline]
    pub fn new(raw_id: u32) -> Option<Self> {
        if raw_id > Self::MAX_ID {
            return None;
        }
        NonZeroU32::new(raw_id).map(Self)
    }

    /// Get the underlying value with Rust's assurances
    /// of non-zero-ness.
    #[inline]
    pub fn get(self) -> NonZeroU32 {
        self.0
    }

    /// Get the underlying value as a convenient primitive
    #[inline]
    pub fn get_raw(self) -> u32 {
        self.0.get()
    }
}

impl From<TracerId> for NonZeroU32 {
    #[inline]
    fn from(t: TracerId) -> Self {
        t.0
    }
}

impl From<TracerId> for u32 {
    #[inline]
    fn from(t: TracerId) -> Self {
        t.0.get()
    }
}

impl TryFrom<u32> for TracerId {
    type Error = InvalidTracerId;
    #[inline]
    fn try_from(raw_id: u32) -> Result<Self, Self::Error> {
        match TracerId::new(raw_id) {
            Some(id) => Ok(id),
            None => Err(InvalidTracerId),
        }
    }
}

macro_rules! infallible_sizing_try_from_impl {
    ($prim_ty:ty, $target_ty:ty, $err_ty:ty, $err_constructor:expr) => {
        impl TryFrom<$prim_ty> for $target_ty {
            type Error = $err_ty;
            #[inline]
            fn try_from(raw_id: $prim_ty) -> Result<Self, Self::Error> {
                match <$target_ty>::new(raw_id.into()) {
                    Some(id) => Ok(id),
                    None => Err($err_constructor),
                }
            }
        }
    };
}

macro_rules! fallible_sizing_try_from_impl {
    ($prim_ty:ty, $target_ty:ty, $err_ty:ty, $err_constructor:expr) => {
        impl TryFrom<$prim_ty> for $target_ty {
            type Error = $err_ty;
            #[inline]
            fn try_from(raw_id: $prim_ty) -> Result<Self, Self::Error> {
                let intermediate_id: u32 = match raw_id.try_into() {
                    Ok(i) => i,
                    Err(_) => return Err($err_constructor),
                };
                match <$target_ty>::new(intermediate_id) {
                    Some(id) => Ok(id),
                    None => Err($err_constructor),
                }
            }
        }
    };
}

infallible_sizing_try_from_impl!(u8, TracerId, InvalidTracerId, InvalidTracerId);
infallible_sizing_try_from_impl!(u16, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(u64, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(u128, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(usize, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(i8, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(i16, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(i32, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(i64, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(i128, TracerId, InvalidTracerId, InvalidTracerId);
fallible_sizing_try_from_impl!(isize, TracerId, InvalidTracerId, InvalidTracerId);

infallible_sizing_try_from_impl!(u8, EventId, InvalidEventId, InvalidEventId);
infallible_sizing_try_from_impl!(u16, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(u64, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(u128, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(usize, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(i8, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(i16, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(i32, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(i64, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(i128, EventId, InvalidEventId, InvalidEventId);
fallible_sizing_try_from_impl!(isize, EventId, InvalidEventId, InvalidEventId);

/// Uniquely identify an event or kind of event.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EventId(NonZeroU32);

impl EventId {
    /// The maximum permissible id value for an Event at all
    ///
    /// This value is different from MAX_USER_ID in order to
    /// support a reserved range of EventIds for protocol use
    pub const MAX_INTERNAL_ID: u32 = 0b0111_1111_1111_1111_1111_1111_1111_1111;
    /// The number of id values that are reserved for use by the
    /// tracer implementation.
    pub const NUM_RESERVED_IDS: u32 = 256;
    /// The maximum-permissable id value for for an Event
    /// defined by end users.
    pub const MAX_USER_ID: u32 = EventId::MAX_INTERNAL_ID - EventId::NUM_RESERVED_IDS;

    /// The tracer produced a log report for transmission to the backend
    /// for external analysis.
    pub const EVENT_PRODUCED_EXTERNAL_REPORT: EventId =
        EventId(unsafe { NonZeroU32::new_unchecked(EventId::MAX_INTERNAL_ID - 1) });
    /// There was not sufficient room in memory to store all desired events or clock data
    pub const EVENT_LOG_OVERFLOWED: EventId =
        EventId(unsafe { NonZeroU32::new_unchecked(EventId::MAX_INTERNAL_ID - 2) });
    /// A logical clock's count reached the maximum trackable value
    pub const EVENT_LOGICAL_CLOCK_OVERFLOWED: EventId =
        EventId(unsafe { NonZeroU32::new_unchecked(EventId::MAX_INTERNAL_ID - 3) });
    /// The local tracing instance (e.g. Ekotrace) did not have enough memory
    /// reserved to store enough logical clocks to track all of the unique
    /// neighbors that attempt to communicate with it.
    pub const EVENT_NUM_CLOCKS_OVERFLOWED: EventId =
        EventId(unsafe { NonZeroU32::new_unchecked(EventId::MAX_INTERNAL_ID - 4) });

    /// The events reserved for internal use
    pub const INTERNAL_EVENTS: &'static [EventId] = &[
        EventId::EVENT_PRODUCED_EXTERNAL_REPORT,
        EventId::EVENT_LOG_OVERFLOWED,
        EventId::EVENT_LOGICAL_CLOCK_OVERFLOWED,
    ];

    /// raw_id must be greater than 0 and less than EventId::MAX_USER_ID
    #[inline]
    pub fn new(raw_id: u32) -> Option<Self> {
        if raw_id > Self::MAX_USER_ID {
            return None;
        }
        NonZeroU32::new(raw_id).map(Self)
    }

    /// Get the underlying value with Rust's assurances
    /// of non-zero-ness.
    #[inline]
    pub fn get(self) -> NonZeroU32 {
        self.0
    }

    /// Get the underlying value as a convenient primitive
    #[inline]
    pub fn get_raw(self) -> u32 {
        self.0.get()
    }
}

impl TryFrom<u32> for EventId {
    type Error = InvalidEventId;
    #[inline]
    fn try_from(raw_id: u32) -> Result<Self, Self::Error> {
        match EventId::new(raw_id) {
            Some(id) => Ok(id),
            None => Err(InvalidEventId),
        }
    }
}

impl From<EventId> for NonZeroU32 {
    #[inline]
    fn from(e: EventId) -> Self {
        e.0
    }
}

impl From<EventId> for u32 {
    #[inline]
    fn from(e: EventId) -> Self {
        e.0.get()
    }
}
