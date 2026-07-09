//! Grid model for signal flow layout.
//!
//! Represents blocks positioned on a 16×8 grid with routing connections.
//! Blocks can span multiple cells (e.g., 3×2 for an EQ widget).
//!
//! Ported from legacy `signal-ui/components/rig_grid/grid_model.rs` — uses
//! `signal::BlockType` directly instead of `signal_control::block::BlockType`.

use signal::BlockType;
use uuid::Uuid;

/// Grid dimensions — 16 columns × 8 rows.
pub const GRID_COLS: usize = 16;
pub const GRID_ROWS: usize = 8;

/// Position on the grid (0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridPosition {
    pub row: usize,
    pub col: usize,
}

impl GridPosition {
    pub const fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }

    pub const fn is_valid(&self) -> bool {
        self.row < GRID_ROWS && self.col < GRID_COLS
    }
}

/// Size of a block on the grid (in cells).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridSize {
    pub width: usize,
    pub height: usize,
}

impl GridSize {
    pub const fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }

    pub const fn single() -> Self {
        Self::new(1, 1)
    }

    pub const fn wide() -> Self {
        Self::new(2, 1)
    }

    pub const fn large() -> Self {
        Self::new(3, 2)
    }

    pub const fn xlarge() -> Self {
        Self::new(4, 2)
    }

    pub const fn square() -> Self {
        Self::new(2, 2)
    }
}

impl Default for GridSize {
    fn default() -> Self {
        Self::single()
    }
}

/// Widget type to render inside a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlockWidget {
    #[default]
    Label,
    EqGraph,
    CompressorGraph,
    GateGraph,
    DelayGraph,
    ReverbGraph,
    AmpCab,
    DriveGraph,
    ModulationGraph,
    Tuner,
    Looper,
}

/// A block placed on the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct GridBlock {
    pub id: Uuid,
    pub name: String,
    pub short_label: String,
    pub block_type: BlockType,
    pub position: GridPosition,
    pub size: GridSize,
    pub widget: BlockWidget,
    pub bypassed: bool,
}

impl GridBlock {
    pub fn new(
        name: impl Into<String>,
        short_label: impl Into<String>,
        block_type: BlockType,
        position: GridPosition,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            short_label: short_label.into(),
            block_type,
            position,
            size: GridSize::single(),
            widget: BlockWidget::Label,
            bypassed: false,
        }
    }

    pub fn with_size(mut self, size: GridSize) -> Self {
        self.size = size;
        self
    }

    pub fn with_widget(mut self, widget: BlockWidget) -> Self {
        self.widget = widget;
        self
    }

    pub fn with_bypassed(mut self, bypassed: bool) -> Self {
        self.bypassed = bypassed;
        self
    }

    pub fn occupies(&self, pos: GridPosition) -> bool {
        pos.col >= self.position.col
            && pos.col < self.position.col + self.size.width
            && pos.row >= self.position.row
            && pos.row < self.position.row + self.size.height
    }
}

/// Connection between grid positions (for routing lines).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridConnection {
    pub from: GridPosition,
    pub to: GridPosition,
}

impl GridConnection {
    pub const fn new(from: GridPosition, to: GridPosition) -> Self {
        Self { from, to }
    }
}

/// Input/output jack on the grid edge.
#[derive(Debug, Clone, PartialEq)]
pub struct GridJack {
    pub label: String,
    pub row: usize,
    pub is_input: bool,
}

impl GridJack {
    pub fn input(label: impl Into<String>, row: usize) -> Self {
        Self {
            label: label.into(),
            row,
            is_input: true,
        }
    }

    pub fn output(label: impl Into<String>, row: usize) -> Self {
        Self {
            label: label.into(),
            row,
            is_input: false,
        }
    }
}

/// The complete grid state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SignalFlowGrid {
    pub blocks: Vec<GridBlock>,
    pub connections: Vec<GridConnection>,
    pub inputs: Vec<GridJack>,
    pub outputs: Vec<GridJack>,
}

impl SignalFlowGrid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_block(&mut self, block: GridBlock) {
        self.blocks.push(block);
    }

    pub fn connect(&mut self, from: GridPosition, to: GridPosition) {
        self.connections.push(GridConnection::new(from, to));
    }

    pub fn add_input(&mut self, label: impl Into<String>, row: usize) {
        self.inputs.push(GridJack::input(label, row));
    }

    pub fn add_output(&mut self, label: impl Into<String>, row: usize) {
        self.outputs.push(GridJack::output(label, row));
    }

    pub fn block_at(&self, pos: GridPosition) -> Option<&GridBlock> {
        self.blocks.iter().find(|b| b.occupies(pos))
    }

    /// Map a `BlockType` to the appropriate widget + grid size.
    pub fn widget_for_block_type(bt: BlockType) -> (BlockWidget, GridSize) {
        match bt {
            BlockType::Eq => (BlockWidget::EqGraph, GridSize::xlarge()),
            BlockType::Compressor => (BlockWidget::CompressorGraph, GridSize::large()),
            BlockType::Gate => (BlockWidget::GateGraph, GridSize::square()),
            BlockType::Delay => (BlockWidget::DelayGraph, GridSize::large()),
            BlockType::Reverb => (BlockWidget::ReverbGraph, GridSize::large()),
            BlockType::Drive | BlockType::Saturator => {
                (BlockWidget::DriveGraph, GridSize::square())
            }
            BlockType::Modulation | BlockType::Trem | BlockType::Pitch => {
                (BlockWidget::ModulationGraph, GridSize::wide())
            }
            BlockType::Amp | BlockType::Cabinet => (BlockWidget::AmpCab, GridSize::square()),
            BlockType::Tuner => (BlockWidget::Tuner, GridSize::single()),
            _ => (BlockWidget::Label, GridSize::single()),
        }
    }

    /// Build a `SignalFlowGrid` from a `SignalChain`, auto-positioning blocks
    /// with appropriate widgets and sizes.
    pub fn from_signal_chain(chain: &signal::SignalChain) -> Self {
        let mut grid = Self::new();
        grid.add_input("In", 0);
        grid.add_output("Out", 0);

        let blocks: Vec<&signal::ModuleBlock> = chain.blocks();
        let mut col: usize = 1; // start after input jack

        for mb in &blocks {
            let (widget, size) = Self::widget_for_block_type(mb.block_type());
            let gb = GridBlock::new(
                mb.label(),
                &mb.label()[..3_usize.min(mb.label().len())],
                mb.block_type(),
                GridPosition::new(0, col),
            )
            .with_size(size)
            .with_widget(widget);
            col += size.width + 1; // block width + 1 col gap
            grid.add_block(gb);
        }

        grid
    }

    /// Build from rig-level data: engines → layers → module chains.
    /// Lays out modules row-by-row with blocks proceeding horizontally.
    pub fn from_engines(engines: &[super::signal_flow_grid_view::EngineGridData]) -> Self {
        let mut grid = Self::new();
        grid.add_input("In", 0);
        grid.add_output("Out", 0);

        let mut row: usize = 0;

        for engine in engines {
            for layer in &engine.layers {
                let mut col: usize = 1;
                for mc in &layer.module_chains {
                    for mb in mc.chain.blocks() {
                        let (widget, size) = Self::widget_for_block_type(mb.block_type());
                        // Clamp to grid bounds
                        if col + size.width > GRID_COLS || row + size.height > GRID_ROWS {
                            continue;
                        }
                        let gb = GridBlock::new(
                            mb.label(),
                            &mb.label()[..3_usize.min(mb.label().len())],
                            mb.block_type(),
                            GridPosition::new(row, col),
                        )
                        .with_size(size)
                        .with_widget(widget);
                        col += size.width + 1;
                        grid.add_block(gb);
                    }
                }
                row += 2; // leave a row gap between layers
            }
        }

        grid
    }
}
