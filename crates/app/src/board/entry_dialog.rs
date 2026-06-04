pub(crate) struct EntryDialog {
    pub(crate) open: bool,
    pub(crate) entry_id: Option<u32>,
    pub(crate) editing: bool,
}

impl EntryDialog {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            entry_id: None,
            editing: false,
        }
    }
}
