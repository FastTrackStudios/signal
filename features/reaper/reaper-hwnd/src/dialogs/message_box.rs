//! MessageBox dialog child window IDs and types.

use crate::ChildId;

/// MessageBox type variants (matching Windows MB_* constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageBoxType {
    /// Type 0: OK button only
    Ok = 0,
    /// Type 1: OK and Cancel buttons
    OkCancel = 1,
    /// Type 2: Abort, Retry, and Ignore buttons
    AbortRetryIgnore = 2,
    /// Type 3: Yes, No, and Cancel buttons
    YesNoCancel = 3,
    /// Type 4: Yes and No buttons
    YesNo = 4,
    /// Type 5: Retry and Cancel buttons
    RetryCancel = 5,
    /// Type 6: Cancel, Try Again, and Continue buttons
    CancelTryContinue = 6,
}

/// MessageBox dialog child window IDs.
pub struct MessageBox;

impl MessageBox {
    /// OK button - Class: Button
    pub const OK: ChildId = ChildId(1);
    /// Cancel button - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Abort button - Class: Button
    pub const ABORT: ChildId = ChildId(3);
    /// Retry button - Class: Button
    pub const RETRY: ChildId = ChildId(4);
    /// Ignore button - Class: Button
    pub const IGNORE: ChildId = ChildId(5);
    /// Yes button - Class: Button
    pub const YES: ChildId = ChildId(6);
    /// No button - Class: Button
    pub const NO: ChildId = ChildId(7);
    /// Try Again button - Class: Button
    pub const TRY_AGAIN: ChildId = ChildId(10);
    /// Continue button - Class: Button
    pub const CONTINUE: ChildId = ChildId(11);
    /// Message text - Class: Static
    pub const MESSAGE_TEXT: ChildId = ChildId(65535);
}
