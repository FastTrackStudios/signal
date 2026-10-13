//! Fixed Lane mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Fixed Lane header button click behaviors
    pub enum FixedLaneHeaderButtonClickBehavior {
        NoAction => (0, "No action"),
        SelectItemsInLane => (1, "Select items in lane"),
        ToggleSelectionOfItemsInLane => (2, "Toggle selection of items in lane"),
        PlayOnlyThisLane => (3, "Play only this lane"),
        PlayNoLanes => (4, "Play no lanes"),
        TogglePlayingThisLane => (5, "Toggle playing this lane"),
        PlayOnlyThisLaneWhileMouseButtonIsPressed => (6, "Play only this lane while mouse button is pressed"),
        RecordIntoThisLane => (7, "Record into this lane"),
        PlayAllLanes => (8, "Play all lanes"),
        CompIntoLane => (9, "Comp into lane"),
        CompIntoANewCopyOfLane => (10, "Comp into a new copy of lane"),
        CompIntoANewEmptyLane => (11, "Comp into a new empty lane"),
        InsertLane => (12, "Insert lane"),
        DeleteLaneIncludingMediaItems => (13, "Delete lane (including media items)"),
        CompIntoANewEmptyLaneAutomaticallyCreatingCompAreas => (14, "Comp into a new empty lane, automatically creating comp areas"),
        CopyEditedMediaItemsToNewLaneAndReComp => (15, "Copy edited media items to new lane and re-comp"),
    }
}

define_behavior_enum! {
    /// Fixed Lane header button double click behaviors
    pub enum FixedLaneHeaderButtonDoubleClickBehavior {
        NoAction => (0, "No action"),
        SelectItemsInLane => (1, "Select items in lane"),
        ToggleSelectionOfItemsInLane => (2, "Toggle selection of items in lane"),
        PlayOnlyThisLane => (3, "Play only this lane"),
        PlayNoLanes => (4, "Play no lanes"),
        TogglePlayingThisLane => (5, "Toggle playing this lane"),
        RecordIntoThisLane => (6, "Record into this lane"),
        PlayAllLanes => (7, "Play all lanes"),
        CompIntoLane => (8, "Comp into lane"),
        CompIntoANewCopyOfLane => (9, "Comp into a new copy of lane"),
        CompIntoANewEmptyLane => (10, "Comp into a new empty lane"),
        InsertLane => (11, "Insert lane"),
        DeleteLaneIncludingMediaItems => (12, "Delete lane (including media items)"),
        CompIntoANewEmptyLaneAutomaticallyCreatingCompAreas => (13, "Comp into a new empty lane, automatically creating comp areas"),
        CopyEditedMediaItemsWithNoMatchingSourceLaneToNewLaneAndReComp => (14, "Copy edited media items with no matching source lane to new lane and re-comp"),
    }
}

define_behavior_enum! {
    /// Fixed Lane linked lane left drag behaviors
    pub enum FixedLaneLinkedLaneLeftDragBehavior {
        NoAction => (0, "No action"),
        MoveCompAreaVertically => (1, "Move comp area vertically"),
        MoveCompAreaIgnoringSnap => (2, "Move comp area ignoring snap"),
        MoveCompAreaHorizontally => (3, "Move comp area horizontally"),
        MoveCompAreaHorizontallyIgnoringSnap => (4, "Move comp area horizontally ignoring snap"),
        MoveCompAreaVerticallyDuplicate => (5, "Move comp area vertically"),
        MoveCompAreaOnOneAxisOnly => (6, "Move comp area on one axis only"),
        MoveCompAreaAndMediaItems => (7, "Move comp area and media items"),
        MoveCompAreaAndMediaItemsIgnoringSnap => (8, "Move comp area and media items ignoring snap"),
        MoveCompAreaAndMediaItemsHorizontally => (9, "Move comp area and media items horizontally"),
        MoveCompAreaAndMediaItemsHorizontallyIgnoringSnap => (10, "Move comp area and media items horizontally ignoring snap"),
        MoveCompAreaAndMediaItemsVertically => (11, "Move comp area and media items vertically"),
        MoveCompAreaAndMediaItemsOnOneAxisOnly => (12, "Move comp area and media items on one axis only"),
        AddCompArea => (13, "Add comp area"),
        AddCompAreaIgnoringSnap => (14, "Add comp area ignoring snap"),
        CopyCompAreaAndMediaItemsTogether => (15, "Copy comp area and media items together"),
        CopyCompAreaAndMediaItemsTogetherIgnoringSnap => (16, "Copy comp area and media items together ignoring snap"),
        MoveCompAreaAndMediaItemsOnAllLanes => (17, "Move comp area and media items on all lanes"),
        MoveCompAreaAndMediaItemsOnAllLanesIgnoringSnap => (18, "Move comp area and media items on all lanes ignoring snap"),
        CopyCompAreaAndMediaItemsOnAllLanesTogether => (19, "Copy comp area and media items on all lanes together"),
        CopyCompAreaAndMediaItemsOnAllLanesTogetherIgnoringSnap => (20, "Copy comp area and media items on all lanes together ignoring snap"),
        MoveCompAreaAndAdjacentCompAreaEdges => (21, "Move comp area and adjacent comp area edges"),
        MoveCompAreaAndAdjacentCompAreaEdgesIgnoringSnap => (22, "Move comp area and adjacent comp area edges ignoring snap"),
        MoveCompAreaAndAdjacentCompAreaEdgesAndMediaItems => (23, "Move comp area and adjacent comp area edges and media items"),
        MoveCompAreaAndAdjacentCompAreaEdgesAndMediaItemsIgnoringSnap => (24, "Move comp area and adjacent comp area edges and media items ignoring snap"),
        MoveCompAreaAndAdjacentCompAreaEdgesHorizontally => (25, "Move comp area and adjacent comp area edges horizontally"),
        MoveCompAreaAndAdjacentCompAreaEdgesHorizontallyIgnoringSnap => (26, "Move comp area and adjacent comp area edges horizontally ignoring snap"),
        MoveCompAreaAndAdjacentCompAreaEdgesOnOneAxisOnly => (27, "Move comp area and adjacent comp area edges on one axis only"),
    }
}

define_behavior_enum! {
    /// Fixed Lane linked lane click behaviors
    pub enum FixedLaneLinkedLaneClickBehavior {
        NoAction => (0, "No action"),
        MoveCompAreaUp => (1, "Move comp area up"),
        MoveCompAreaDown => (2, "Move comp area down"),
        DeleteCompAreaButNotMediaItems => (3, "Delete comp area but not media items"),
        SetLoopPointsToCompArea => (4, "Set loop points to comp area"),
        MoveCompAreaToLaneUnderMouse => (5, "Move comp area to lane under mouse"),
        HealCompAreaWithAdjacentCompAreasOnTheSameLane => (6, "Heal comp area with adjacent comp areas on the same lane"),
        SplitMediaItemsAtCompAreaEdges => (7, "Split media items at comp area edges"),
        ExtendCompAreaToNextCompAreaOrEndOfMedia => (8, "Extend comp area to next comp area or end of media"),
        DeleteCompArea => (9, "Delete comp area"),
        SplitCompArea => (10, "Split comp area"),
        SplitCompAreaIgnoringSnap => (11, "Split comp area ignoring snap"),
        SetLoopPointsToCompAreaHalfSecondPrerollPostroll => (12, "Set loop points to comp area (half second preroll/postroll)"),
        SetLoopPointsToCompAreaOneSecondPrerollPostroll => (13, "Set loop points to comp area (one second preroll/postroll)"),
        CopyEditedMediaItemsToNewLaneAndReComp => (14, "Copy edited media items to new lane and re-comp"),
    }
}

define_behavior_enum! {
    /// Fixed Lane linked lane double click behaviors
    pub enum FixedLaneLinkedLaneDoubleClickBehavior {
        NoAction => (0, "No action"),
        SetLoopPointsToCompArea => (1, "Set loop points to comp area"),
        ExtendCompAreaToNextCompAreaOrEndOfMedia => (2, "Extend comp area to next comp area or end of media"),
        SplitMediaItemsAtCompAreaEdges => (3, "Split media items at comp area edges"),
        CopyEditedMediaItemsToNewLaneAndReComp => (4, "Copy edited media items to new lane and re-comp"),
        SetLoopPointsToCompAreaHalfSecondPrerollPostroll => (5, "Set loop points to comp area (half second preroll/postroll)"),
        SetLoopPointsToCompAreaOneSecondPrerollPostroll => (6, "Set loop points to comp area (one second preroll/postroll)"),
    }
}
