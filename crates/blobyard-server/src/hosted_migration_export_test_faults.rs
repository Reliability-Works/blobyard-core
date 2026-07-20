//! Deterministic export I/O failures used only by unit tests.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExportFault {
    ApiTransport,
    FetchClient,
    TemporaryDirectory,
    CreateFile,
    WriteFile,
    FlushFile,
    SyncFile,
}

thread_local! {
    static EXPORT_FAULT: std::cell::Cell<Option<ExportFault>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
pub(super) struct FaultGuard;

#[cfg(test)]
impl Drop for FaultGuard {
    fn drop(&mut self) {
        EXPORT_FAULT.with(|slot| slot.set(None));
    }
}

#[cfg(test)]
pub(super) fn activate(fault: ExportFault) -> FaultGuard {
    EXPORT_FAULT.with(|slot| slot.set(Some(fault)));
    FaultGuard
}

pub(super) fn active(fault: ExportFault) -> bool {
    EXPORT_FAULT.with(|slot| slot.get() == Some(fault))
}
