//! Errors exposed in the ModalityProbe API.
//!
//! In order to be appropriate for embedded
//! use, these errors should be as tiny
//! and precise as possible.

/// Error that indicates an invalid event id was detected.
///
///
/// event ids must be greater than 0 and less than EventId::MAX_USER_ID
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InvalidEventId;

/// Error that indicates an invalid probe id was detected.
///
///
/// probe ids must be greater than 0 and less than ProbeId::MAX_ID
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InvalidProbeId;

/// An error relating to the initialization
/// of an ModalityProbe instance from parts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InitializationError {
    /// A provided primitive, unvalidated probe id
    /// turned out to be invalid.
    InvalidProbeId,
    /// A problem with the backing memory setup.
    StorageSetupError(StorageSetupError),
}

/// An error relating to the initialization
/// of in-memory probe storage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StorageSetupError {
    /// The provided storage space was insufficient
    /// to support a minimally useful probe
    /// implementation.
    UnderMinimumAllowedSize,
    /// The provided space for clock-buckets and logging
    /// exceeded the number of units the probe implementation
    /// can track.
    ExceededMaximumAddressableSize,
    /// The provided or computed output pointer for
    /// probe data storage turned out to be null.
    NullDestination,
}

/// The errors than can occur when producing a probe's
/// causal history for use by some other probe instance.
///
/// Returned in the error cases for the `produce_snapshot` and
/// `produce_snapshot_bytes` functions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProduceError {
    /// The destination that is receiving the history is not big enough.
    ///
    /// Indicates that the end user should provide a larger destination buffer.
    InsufficientDestinationSize,
}

/// The errors than can occur when merging in the causal history from some
/// other probe instance.
///
/// Returned in the error cases for the `merge_snapshot` and
/// `merge_fixed_size_snapshot` functions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MergeError {
    /// The local probe does not have enough space to track all
    /// of direct neighbors attempting to communicate with it.
    ExceededAvailableClocks,
    /// The the external history source buffer we attempted to merge
    /// was insufficiently sized for a valid causal snapshot.
    InsufficientSourceSize,
    /// The external history violated a semantic rule of the protocol,
    /// such as by having a probe_id out of the allowed value range.
    ExternalHistorySemantics,
}
/// The error relating to using the `report` method to
/// produce a full causal history log report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReportError {
    /// The destination that is receiving the report is not big enough.
    ///
    /// Indicates that the end user should provide a larger destination buffer.
    InsufficientDestinationSize,
}

/// General purpose error that captures all errors that arise
/// from using the ModalityProbe APIs.
///
/// Not directly returned by any of the public APIs, but provided
/// as a convenience for catch-all error piping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalityProbeError {
    /// Error that indicates an invalid event id was detected.
    ///
    /// Event ids must be greater than 0 and less than EventId::MAX_USER_ID
    InvalidEventId,
    /// Error that indicates an invalid probe id was detected.
    ///
    /// Probe ids must be greater than 0 and less than ProbeId::MAX_ID
    InvalidProbeId,
    /// An error relating to the initialization of an ModalityProbe instance.
    InitializationError(InitializationError),
    /// The errors than can occur when using the `produce_snapshot`
    /// and `produce_snapshot_bytes` functions.
    ProduceError(ProduceError),
    /// The errors than can occur when using the `merge_snapshot`
    /// and `merge_fixed_size_snapshot` functions.
    MergeError(MergeError),
    /// The error relating to using the `report` method to
    /// produce a full causal history log report.
    ReportError(ReportError),
}

impl From<InvalidEventId> for ModalityProbeError {
    #[inline]
    fn from(_: InvalidEventId) -> Self {
        ModalityProbeError::InvalidEventId
    }
}

impl From<InvalidProbeId> for ModalityProbeError {
    #[inline]
    fn from(_: InvalidProbeId) -> Self {
        ModalityProbeError::InvalidProbeId
    }
}

impl From<InitializationError> for ModalityProbeError {
    #[inline]
    fn from(e: InitializationError) -> Self {
        ModalityProbeError::InitializationError(e)
    }
}

impl From<ProduceError> for ModalityProbeError {
    #[inline]
    fn from(e: ProduceError) -> Self {
        ModalityProbeError::ProduceError(e)
    }
}

impl From<MergeError> for ModalityProbeError {
    #[inline]
    fn from(e: MergeError) -> Self {
        ModalityProbeError::MergeError(e)
    }
}

impl From<ReportError> for ModalityProbeError {
    #[inline]
    fn from(e: ReportError) -> Self {
        ModalityProbeError::ReportError(e)
    }
}

impl From<StorageSetupError> for ModalityProbeError {
    #[inline]
    fn from(e: StorageSetupError) -> Self {
        ModalityProbeError::InitializationError(InitializationError::StorageSetupError(e))
    }
}
