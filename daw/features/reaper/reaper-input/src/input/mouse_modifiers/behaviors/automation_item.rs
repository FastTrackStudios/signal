//! Automation Item mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Automation Item left drag behaviors
    pub enum AutomationItemLeftDragBehavior {
        NoAction => (0, "No action"),
        MoveAutomationItemJustSnap => (1, "Move automation item - just snap"),
        MoveAutomationItemIgnoringSnap => (2, "Move automation item - ignoring snap"),
        CopyAutomationItemAndPoolIt => (3, "Copy automation item and pool it"),
        CopyAutomationItemAndPoolItIgnoringSnap => (4, "Copy automation item and pool it, ignoring snap"),
        CopyAutomationItemJustCopy => (5, "Copy automation item - just copy"),
        CopyAutomationItemIgnoringSnap => (6, "Copy automation item - ignoring snap"),
        CopyAutomationItemAndPoolItIgnoringTimeSelection => (7, "Copy automation item and pool it, ignoring time selection"),
        CopyAutomationItemAndPoolItIgnoringSnapAndTimeSelection => (8, "Copy automation item and pool it, ignoring snap and time selection"),
        CopyAutomationItemIgnoringTimeSelection => (9, "Copy automation item - ignoring time selection"),
    }
}

define_behavior_enum! {
    /// Automation Item edge behaviors
    pub enum AutomationItemEdgeBehavior {
        NoAction => (0, "No action"),
        MoveAutomationItemEdgeJustMove => (1, "Move automation item edge - just move"),
        MoveAutomationItemEdgeIgnoringSnap => (2, "Move automation item edge - ignoring snap"),
        StretchAutomationItemEdge => (3, "Stretch automation item edge"),
        StretchAutomationItemEdgeIgnoringSnap => (4, "Stretch automation item edge - ignoring snap"),
        CollectPointsIntoAutomationItem => (5, "Collect points into automation item"),
        CollectPointsIntoAutomationItemIgnoringSnap => (6, "Collect points into automation item - ignoring snap"),
        StretchAutomationItemEdgeRelativeToOtherSelectedItems => (7, "Stretch automation item edge - relative to other selected items"),
        StretchAutomationItemEdgeRelativeToOtherSelectedItemsIgnoringSnap => (8, "Stretch automation item edge - relative to other selected items ignoring snap"),
        MoveAutomationItemEdgeRelativeToOtherSelectedItems => (9, "Move automation item edge - relative to other selected items"),
        MoveAutomationItemEdgeRelativeToOtherSelectedItemsIgnoringSnap => (10, "Move automation item edge - relative to other selected items ignoring snap"),
    }
}

define_behavior_enum! {
    /// Automation Item double click behaviors
    pub enum AutomationItemDoubleClickBehavior {
        NoAction => (0, "No action"),
        ShowAutomationItemProperties => (1, "Show automation item properties"),
        SetTimeSelectionToItem => (2, "Set time selection to item"),
        SetLoopPointsToItem => (3, "Set loop points to item"),
        LoadAutomationItem => (4, "Load automation item"),
    }
}
