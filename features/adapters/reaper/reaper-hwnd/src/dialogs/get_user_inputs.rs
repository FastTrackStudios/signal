//! GetUserInputs dialog child window IDs.

use crate::ChildId;

/// GetUserInputs dialog child window IDs.
///
/// The GetUserInputs API creates a dialog with a variable number of input fields.
/// Field IDs start at 1100 and increment for each additional field.
pub struct GetUserInputs;

impl GetUserInputs {
    /// OK button - Class: Button
    pub const OK: ChildId = ChildId(1);
    /// Cancel button - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Dialog title/description - Class: Static
    pub const TITLE: ChildId = ChildId(1000);
    /// First input field - Class: Edit
    pub const INPUT_1: ChildId = ChildId(1100);
    /// Second input field - Class: Edit
    pub const INPUT_2: ChildId = ChildId(1101);
    /// Third input field - Class: Edit
    pub const INPUT_3: ChildId = ChildId(1102);
    /// Fourth input field - Class: Edit
    pub const INPUT_4: ChildId = ChildId(1103);
    /// Fifth input field - Class: Edit
    pub const INPUT_5: ChildId = ChildId(1104);
    /// Sixth input field - Class: Edit
    pub const INPUT_6: ChildId = ChildId(1105);
    /// Seventh input field - Class: Edit
    pub const INPUT_7: ChildId = ChildId(1106);
    /// Eighth input field - Class: Edit
    pub const INPUT_8: ChildId = ChildId(1107);
    /// Ninth input field - Class: Edit
    pub const INPUT_9: ChildId = ChildId(1108);
    /// Tenth input field - Class: Edit
    pub const INPUT_10: ChildId = ChildId(1109);
    /// Eleventh input field - Class: Edit
    pub const INPUT_11: ChildId = ChildId(1110);
    /// Twelfth input field - Class: Edit
    pub const INPUT_12: ChildId = ChildId(1111);
    /// Thirteenth input field - Class: Edit
    pub const INPUT_13: ChildId = ChildId(1112);
    /// Fourteenth input field - Class: Edit
    pub const INPUT_14: ChildId = ChildId(1113);
    /// Fifteenth input field - Class: Edit
    pub const INPUT_15: ChildId = ChildId(1114);
    /// Sixteenth input field - Class: Edit
    pub const INPUT_16: ChildId = ChildId(1115);

    /// Returns the `ChildId` for the Nth input field (1-indexed, max 16).
    pub const fn input_field(n: u32) -> ChildId {
        assert!(n >= 1 && n <= 16, "GetUserInputs supports 1-16 fields");
        ChildId(1099 + n)
    }

    /// First label - Class: Static
    pub const LABEL_1: ChildId = ChildId(1200);
    /// Second label - Class: Static
    pub const LABEL_2: ChildId = ChildId(1201);
    /// Third label - Class: Static
    pub const LABEL_3: ChildId = ChildId(1202);
    /// Fourth label - Class: Static
    pub const LABEL_4: ChildId = ChildId(1203);
    /// Fifth label - Class: Static
    pub const LABEL_5: ChildId = ChildId(1204);
    /// Sixth label - Class: Static
    pub const LABEL_6: ChildId = ChildId(1205);
    /// Seventh label - Class: Static
    pub const LABEL_7: ChildId = ChildId(1206);
    /// Eighth label - Class: Static
    pub const LABEL_8: ChildId = ChildId(1207);
    /// Ninth label - Class: Static
    pub const LABEL_9: ChildId = ChildId(1208);
    /// Tenth label - Class: Static
    pub const LABEL_10: ChildId = ChildId(1209);
    /// Eleventh label - Class: Static
    pub const LABEL_11: ChildId = ChildId(1210);
    /// Twelfth label - Class: Static
    pub const LABEL_12: ChildId = ChildId(1211);
    /// Thirteenth label - Class: Static
    pub const LABEL_13: ChildId = ChildId(1212);
    /// Fourteenth label - Class: Static
    pub const LABEL_14: ChildId = ChildId(1213);
    /// Fifteenth label - Class: Static
    pub const LABEL_15: ChildId = ChildId(1214);
    /// Sixteenth label - Class: Static
    pub const LABEL_16: ChildId = ChildId(1215);

    /// Returns the `ChildId` for the Nth label (1-indexed, max 16).
    pub const fn label_field(n: u32) -> ChildId {
        assert!(n >= 1 && n <= 16, "GetUserInputs supports 1-16 fields");
        ChildId(1199 + n)
    }
}
