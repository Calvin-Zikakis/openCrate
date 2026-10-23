//! Crash injection. The executor reports each point just before or after an external effect; a
//! test chooses where to "lose power" and the run stops there, leaving disk and store exactly as
//! a real crash would.

/// Where the executor can be interrupted, in the order they occur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FaultPoint {
    BeforeIntentSaved,
    AfterIntentSaved,
    BeforeLink,
    AfterLink,
    BeforeRecordCommit,
    AfterRecordCommit,
    BeforeRemoveStaged,
    AfterRemoveStaged,
    /// While copying into publication staging: the `.part` file is half written.
    MidCopy,
    /// The copy is complete and verified but not yet renamed to its staged name.
    BeforeCopyRename,
}

impl FaultPoint {
    /// The points of the publish protocol, excluding copying.
    pub const PUBLISH: [Self; 8] = [
        Self::BeforeIntentSaved,
        Self::AfterIntentSaved,
        Self::BeforeLink,
        Self::AfterLink,
        Self::BeforeRecordCommit,
        Self::AfterRecordCommit,
        Self::BeforeRemoveStaged,
        Self::AfterRemoveStaged,
    ];
    pub const COPY: [Self; 2] = [Self::MidCopy, Self::BeforeCopyRename];
}

#[derive(Debug)]
pub struct Crash(pub FaultPoint);

pub trait Faults {
    /// Return `Err` to stop here as if the process died.
    fn point(&mut self, point: FaultPoint) -> Result<(), Crash>;
}

/// Production: never interrupts.
pub struct NoFaults;

impl Faults for NoFaults {
    fn point(&mut self, _: FaultPoint) -> Result<(), Crash> {
        Ok(())
    }
}

impl<F: FnMut(FaultPoint) -> Result<(), Crash>> Faults for F {
    fn point(&mut self, point: FaultPoint) -> Result<(), Crash> {
        self(point)
    }
}
