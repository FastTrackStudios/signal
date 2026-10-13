const UI_WIDTH = 460;
const UI_HEIGHT = 680;

type ExportOptions = {
  includeSvg: boolean;
  includePng: boolean;
  maxDepth: number;
};

const CORE10_FRAME_NAMES = [
  'parity/primitives/fractional-geometry',
  'parity/paint/gradient-matrix',
  'parity/paint/blend-mode-stack',
  'parity/effects/layer-background-blur',
  'parity/strokes/caps-joins-dashes',
  'parity/masks/vector-text-image-mask',
  'parity/images/fill-mode-matrix',
  'parity/text/mixed-style-runs',
  'parity/layout/auto-layout-hug-fill',
  'parity/regression/kitchen-sink',
] as const;

type ExportNode = {
  id: string;
  name: string;
  type: string;
  visible: boolean;
  locked?: boolean;
  opacity?: number;
  blendMode?: string;
  absoluteBoundingBox?: RectLike;
  relativeTransform?: number[][];
  size?: SizeLike;
  rotation?: number;
  constraints?: unknown;
  clipsContent?: boolean;
  layoutMode?: string;
  layoutWrap?: string;
  layoutSizingHorizontal?: string;
  layoutSizingVertical?: string;
  layoutAlign?: string;
  layoutGrow?: number;
  primaryAxisSizingMode?: string;
  primaryAxisAlignItems?: string;
  counterAxisSizingMode?: string;
  counterAxisAlignItems?: string;
  paddingTop?: number;
  paddingRight?: number;
  paddingBottom?: number;
  paddingLeft?: number;
  itemSpacing?: number;
  effects?: unknown[];
  fills?: unknown[];
  strokes?: unknown[];
  strokeWeight?: number;
  strokeMiterLimit?: number;
  strokeAlign?: string;
  strokeJoin?: string;
  strokeCap?: string;
  dashPattern?: number[];
  isMask?: boolean;
  maskType?: string;
  cornerSmoothing?: number;
  cornerRadius?: number;
  topLeftRadius?: number;
  topRightRadius?: number;
  bottomLeftRadius?: number;
  bottomRightRadius?: number;
  rectangleCornerRadii?: number[];
  fillGeometry?: unknown;
  strokeGeometry?: unknown;
  characters?: string;
  fontSize?: number;
  fontFamily?: string;
  fontWeight?: number;
  lineHeightPx?: number;
  letterSpacingPx?: number;
  textAlignHorizontal?: string;
  textAlignVertical?: string;
  textAutoResize?: string;
  lineHeight?: unknown;
  letterSpacing?: unknown;
  paragraphSpacing?: number;
  paragraphIndent?: number;
  textCase?: string;
  textDecoration?: string;
  textStyleId?: string;
  componentProperties?: unknown;
  componentPropertyReferences?: unknown;
  explicitVariableModes?: unknown;
  boundVariables?: unknown;
  pluginData?: unknown;
  sharedPluginData?: unknown;
  componentId?: string;
  componentSetId?: string;
  variantProperties?: Record<string, string>;
  exports?: {
    svgBase64?: string;
    pngBase64?: string;
  };
  assetRefs?: {
    svg?: string;
    png?: string;
  };
  imageFillAssetRefs?: Record<string, string>;
  children?: ExportNode[];
};

type RectLike = {
  x: number;
  y: number;
  width: number;
  height: number;
};

type SizeLike = {
  x: number;
  y: number;
};

type ExportPayload = {
  schema: 'fts.figma.export/v1' | 'fts.figma.export/v2';
  generatedAt: string;
  source: {
    plugin: string;
    pluginVersion: string;
    editorType: string;
    fileKey?: string;
    page: { id: string; name: string };
  };
  selection: {
    ids: string[];
    names: string[];
    totalRoots: number;
    totalNodes: number;
  };
  options: ExportOptions;
  assets?: ExportAsset[];
  nodes: ExportNode[];
};

type Core10FrameStatus = {
  required: string[];
  found: string[];
  missing: string[];
  extraParityFrames: string[];
};

type ExportAssetKind = 'svg' | 'png' | 'image-fill';

type ExportAsset = {
  id: string;
  kind: ExportAssetKind;
  mime: string;
  encoding: 'base64';
  data: string;
  byteLength: number;
};

type AssetAccumulator = {
  assets: ExportAsset[];
  byHash: Map<string, string>;
};

type ExportRoot = SceneNode;

let liveSyncEnabled = false;
let exportQueued = false;
let bridgeLiveEnabled = true;
let bridgeLiveUrl = 'http://localhost:43123/figma-export';
let pinnedExportRootIds: string[] = [];
let hasManualRootSelection = false;

figma.showUI(__html__, { width: UI_WIDTH, height: UI_HEIGHT, themeColors: true });
postSelectionSummary();
maybeAutoPinMainFrame();
postPinnedRootsSummary();
postFrameCatalog();

figma.on('selectionchange', () => {
  postSelectionSummary();
  if (liveSyncEnabled) {
    queueExport();
  }
});

figma.on('documentchange', () => {
  prunePinnedRoots();
  maybeAutoPinMainFrame();
  postFrameCatalog();
  postPinnedRootsSummary();
  if (liveSyncEnabled) {
    queueExport();
  }
});

figma.ui.onmessage = async (msg: unknown) => {
  if (!msg || typeof msg !== 'object') {
    return;
  }

  const message = msg as {
    type?: string;
    options?: Partial<ExportOptions>;
    enabled?: boolean;
    bridgeUrl?: string;
    bridgeEnabled?: boolean;
    nodeId?: string;
    parityRootDir?: string;
  };

  if (message.type === 'close') {
    figma.closePlugin();
    return;
  }

  if (message.type === 'set-live') {
    liveSyncEnabled = Boolean(message.enabled);
    figma.notify(liveSyncEnabled ? 'Live export enabled' : 'Live export paused');
    if (liveSyncEnabled) {
      queueExport();
    }
    return;
  }

  if (message.type === 'export-now') {
    const options = normalizeOptions(message.options);
    await exportSelection(options, 'manual');
    return;
  }

  if (message.type === 'export-to-bridge') {
    const options = normalizeOptions(message.options);
    await exportToBridge(options, message.bridgeUrl);
    return;
  }

  if (message.type === 'set-live-bridge') {
    bridgeLiveEnabled = Boolean(message.bridgeEnabled);
    if (typeof message.bridgeUrl === 'string' && message.bridgeUrl.trim().length > 0) {
      bridgeLiveUrl = message.bridgeUrl.trim();
    }
    figma.notify(bridgeLiveEnabled ? 'Live bridge enabled' : 'Live bridge disabled');
    if (liveSyncEnabled && bridgeLiveEnabled) {
      queueExport();
    }
    return;
  }

  if (message.type === 'set-export-roots-from-selection') {
    hasManualRootSelection = true;
    const roots = rootsFromSelection();
    if (roots.length === 0) {
      figma.notify('Select at least one frame before pinning export roots.');
      figma.ui.postMessage({
        type: 'export-error',
        message: 'Select at least one frame before pinning export roots.',
      });
      return;
    }
    pinnedExportRootIds = roots.map((node) => node.id);
    postPinnedRootsSummary();
    postFrameCatalog();
    figma.notify(`Pinned ${roots.length} export root(s).`);
    if (liveSyncEnabled) {
      queueExport();
    }
    return;
  }

  if (message.type === 'set-export-root-by-id') {
    hasManualRootSelection = true;
    const nodeId = message.nodeId?.trim();
    if (!nodeId) {
      return;
    }
    const node = figma.getNodeById(nodeId);
    if (!node || node.type !== 'FRAME') {
      figma.notify('Selected node is no longer a frame.');
      postFrameCatalog();
      return;
    }
    pinnedExportRootIds = [node.id];
    postPinnedRootsSummary();
    postFrameCatalog();
    figma.notify(`Pinned frame: ${node.name}`);
    if (liveSyncEnabled) {
      queueExport();
    }
    return;
  }

  if (message.type === 'clear-export-roots') {
    hasManualRootSelection = true;
    pinnedExportRootIds = [];
    postPinnedRootsSummary();
    postFrameCatalog();
    figma.notify('Cleared pinned export roots.');
    return;
  }

  if (message.type === 'refresh-frames') {
    prunePinnedRoots();
    maybeAutoPinMainFrame();
    postFrameCatalog();
    postPinnedRootsSummary();
    return;
  }

  if (message.type === 'validate-core10') {
    const status = getCore10FrameStatus();
    figma.ui.postMessage({
      type: 'core10-validation-result',
      ...status,
    });
    return;
  }

  if (message.type === 'export-core10-to-bridge') {
    const options = normalizeOptions(message.options);
    await exportCore10ToBridge(options, message.bridgeUrl, message.parityRootDir);
    return;
  }

  if (message.type === 'export-all-parity-to-bridge') {
    const options = normalizeOptions(message.options);
    await exportAllParityToBridge(options, message.bridgeUrl, message.parityRootDir);
    return;
  }

  if (message.type === 'generate-core10-scenarios') {
    await generateCore10Scenarios();
    figma.ui.postMessage({
      type: 'core10-generated',
      count: CORE10_FRAME_NAMES.length,
    });
    postFrameCatalog();
    postPinnedRootsSummary();
    postSelectionSummary();
    return;
  }
};

async function generateCore10Scenarios(): Promise<void> {
  const page = figma.currentPage;
  const existingFrames = page.findAll((n) => n.type === 'FRAME') as FrameNode[];
  const byName = new Map(existingFrames.map((frame) => [frame.name.trim(), frame]));
  const generated: FrameNode[] = [];

  for (let i = 0; i < CORE10_FRAME_NAMES.length; i += 1) {
    const name = CORE10_FRAME_NAMES[i];
    const frame = byName.get(name) ?? figma.createFrame();
    if (!frame.parent) {
      page.appendChild(frame);
    }
    frame.name = name;
    frame.resizeWithoutConstraints(1920, 1080);
    frame.fills = [{ type: 'SOLID', color: { r: 0.96, g: 0.96, b: 0.96 } }];
    frame.x = (i % 2) * 1980;
    frame.y = Math.floor(i / 2) * 1120;
    for (const child of [...frame.children]) {
      child.remove();
    }

    await addScenarioSkeleton(frame, name);
    generated.push(frame);
  }

  figma.currentPage.selection = generated;
  figma.viewport.scrollAndZoomIntoView(generated);
  figma.notify(`Generated ${generated.length} Core scenario frame(s).`);
}

async function addScenarioSkeleton(frame: FrameNode, frameName: string): Promise<void> {
  await addLabel(frame, frameName, 24, 24, 20);
  await addLabel(frame, 'Row A: baseline', 24, 72);
  await addLabel(frame, 'Row B: edge', 24, 384);
  await addLabel(frame, 'Row C: stress', 24, 696);

  switch (frameName) {
    case 'parity/primitives/fractional-geometry':
      buildFractionalGeometry(frame);
      return;
    case 'parity/paint/gradient-matrix':
      buildGradientMatrix(frame);
      return;
    case 'parity/paint/blend-mode-stack':
      buildBlendModeStack(frame);
      return;
    case 'parity/effects/layer-background-blur':
      buildBlurMatrix(frame);
      return;
    case 'parity/strokes/caps-joins-dashes':
      buildStrokeMatrix(frame);
      return;
    case 'parity/masks/vector-text-image-mask':
      await buildMaskMatrix(frame);
      return;
    case 'parity/images/fill-mode-matrix':
      buildImageFillMatrix(frame);
      return;
    case 'parity/text/mixed-style-runs':
      await buildMixedTextRuns(frame);
      return;
    case 'parity/layout/auto-layout-hug-fill':
      await buildAutoLayoutMatrix(frame);
      return;
    case 'parity/regression/kitchen-sink':
      await buildKitchenSink(frame);
      return;
    default:
      return;
  }
}

async function addLabel(
  parent: ChildrenMixin,
  text: string,
  x: number,
  y: number,
  size = 12,
): Promise<TextNode> {
  const node = figma.createText();
  await figma.loadFontAsync(node.fontName as FontName);
  node.characters = text;
  node.fontSize = size;
  node.x = x;
  node.y = y;
  parent.appendChild(node);
  return node;
}

function makeRect(
  parent: ChildrenMixin,
  x: number,
  y: number,
  w: number,
  h: number,
  color: RGB = { r: 0.8, g: 0.8, b: 0.8 },
): RectangleNode {
  const rect = figma.createRectangle();
  rect.resizeWithoutConstraints(w, h);
  rect.x = x;
  rect.y = y;
  rect.fills = [{ type: 'SOLID', color }];
  parent.appendChild(rect);
  return rect;
}

function buildFractionalGeometry(frame: FrameNode): void {
  const a = makeRect(frame, 180.5, 120.5, 160.5, 80.5, { r: 0.22, g: 0.47, b: 0.96 });
  a.strokes = [{ type: 'SOLID', color: { r: 0.08, g: 0.16, b: 0.32 } }];
  a.strokeWeight = 1;

  const b = figma.createEllipse();
  b.resizeWithoutConstraints(120.5, 120.5);
  b.x = 390.5;
  b.y = 110.5;
  b.fills = [{ type: 'SOLID', color: { r: 0.96, g: 0.45, b: 0.22 } }];
  frame.appendChild(b);

  const c = figma.createLine();
  c.resizeWithoutConstraints(220.5, 0);
  c.x = 560.5;
  c.y = 165.5;
  c.strokes = [{ type: 'SOLID', color: { r: 0.11, g: 0.11, b: 0.11 } }];
  c.strokeWeight = 1;
  frame.appendChild(c);

  makeRect(frame, 180.5, 430.25, 120.25, 120.25, { r: 0.17, g: 0.73, b: 0.55 });
  makeRect(frame, 315.75, 430.75, 120.25, 120.25, { r: 0.93, g: 0.79, b: 0.27 });
  makeRect(frame, 451.5, 431.1, 120.25, 120.25, { r: 0.91, g: 0.31, b: 0.31 });

  makeRect(frame, 180.5, 744.5, 220.5, 130.5, { r: 0.5, g: 0.4, b: 0.9 }).rotation = 7;
  makeRect(frame, 330.25, 770.25, 220.5, 130.5, { r: 0.2, g: 0.8, b: 0.8 }).rotation = -5;
}

function buildGradientMatrix(frame: FrameNode): void {
  const linear = makeRect(frame, 180, 110, 280, 180);
  linear.fills = [
    {
      type: 'GRADIENT_LINEAR',
      gradientStops: [
        { position: 0, color: { r: 1, g: 0.2, b: 0.2, a: 1 } },
        { position: 1, color: { r: 0.2, g: 0.2, b: 1, a: 1 } },
      ],
      gradientTransform: [[1, 0, 0], [0, 1, 0]],
    },
  ];

  const radial = makeRect(frame, 500, 110, 280, 180);
  radial.fills = [
    {
      type: 'GRADIENT_RADIAL',
      gradientStops: [
        { position: 0, color: { r: 0.95, g: 0.95, b: 0.2, a: 1 } },
        { position: 1, color: { r: 0.2, g: 0.2, b: 0.2, a: 1 } },
      ],
      gradientTransform: [[1, 0, 0], [0, 1, 0]],
    },
  ];

  const angular = makeRect(frame, 820, 110, 280, 180);
  angular.fills = [
    {
      type: 'GRADIENT_ANGULAR',
      gradientStops: [
        { position: 0, color: { r: 0.3, g: 0.8, b: 0.5, a: 1 } },
        { position: 0.5, color: { r: 0.9, g: 0.4, b: 0.8, a: 1 } },
        { position: 1, color: { r: 0.3, g: 0.8, b: 0.5, a: 1 } },
      ],
      gradientTransform: [[1, 0, 0], [0, 1, 0]],
    },
  ];

  makeRect(frame, 180, 430, 920, 180, { r: 0.1, g: 0.1, b: 0.1 }).opacity = 0.06;
  makeRect(frame, 210, 460, 240, 120, { r: 0.95, g: 0.3, b: 0.3 });
  makeRect(frame, 330, 500, 240, 120, { r: 0.3, g: 0.3, b: 0.95 });

  const stress = makeRect(frame, 180, 744, 920, 180);
  stress.fills = linear.fills;
  stress.strokes = [{ type: 'SOLID', color: { r: 0.05, g: 0.05, b: 0.2 } }];
  stress.strokeWeight = 8;
}

function buildBlendModeStack(frame: FrameNode): void {
  const bg = makeRect(frame, 180, 110, 920, 800, { r: 0.95, g: 0.95, b: 0.98 });
  bg.cornerRadius = 20;
  const r1 = makeRect(frame, 280, 180, 300, 220, { r: 1, g: 0.2, b: 0.2 });
  r1.opacity = 0.75;
  const r2 = makeRect(frame, 420, 250, 300, 220, { r: 0.2, g: 0.6, b: 1 });
  r2.opacity = 0.75;
  r2.blendMode = 'MULTIPLY';
  const r3 = makeRect(frame, 560, 320, 300, 220, { r: 0.3, g: 1, b: 0.4 });
  r3.opacity = 0.75;
  r3.blendMode = 'SCREEN';
}

function buildBlurMatrix(frame: FrameNode): void {
  makeRect(frame, 180, 110, 920, 800, { r: 0.88, g: 0.9, b: 0.93 });
  const a = makeRect(frame, 240, 160, 220, 140, { r: 0.2, g: 0.45, b: 0.9 });
  a.effects = [{ type: 'LAYER_BLUR', blurType: 'NORMAL', radius: 2, visible: true }];
  const b = makeRect(frame, 510, 160, 220, 140, { r: 0.2, g: 0.45, b: 0.9 });
  b.effects = [{ type: 'LAYER_BLUR', blurType: 'NORMAL', radius: 8, visible: true }];
  const c = makeRect(frame, 780, 160, 220, 140, { r: 0.2, g: 0.45, b: 0.9 });
  c.effects = [{ type: 'LAYER_BLUR', blurType: 'NORMAL', radius: 24, visible: true }];

  const glass = makeRect(frame, 320, 430, 640, 360, { r: 1, g: 1, b: 1 });
  glass.opacity = 0.2;
  glass.cornerRadius = 24;
  glass.effects = [{ type: 'BACKGROUND_BLUR', blurType: 'NORMAL', radius: 18, visible: true }];
}

function makeOpenVPath(parent: ChildrenMixin, x: number, y: number): VectorNode {
  const v = figma.createVector();
  v.vectorPaths = [{ windingRule: 'NONZERO', data: 'M 0 120 L 80 0 L 160 120' }];
  v.x = x;
  v.y = y;
  v.fills = [];
  v.strokes = [{ type: 'SOLID', color: { r: 0.08, g: 0.08, b: 0.08 } }];
  v.strokeWeight = 20;
  parent.appendChild(v);
  return v;
}

function buildStrokeMatrix(frame: FrameNode): void {
  const miter = makeOpenVPath(frame, 200, 150);
  miter.strokeJoin = 'MITER';
  const round = makeOpenVPath(frame, 480, 150);
  round.strokeJoin = 'ROUND';
  const bevel = makeOpenVPath(frame, 760, 150);
  bevel.strokeJoin = 'BEVEL';

  const dash1 = figma.createLine();
  dash1.resizeWithoutConstraints(780, 0);
  dash1.x = 220;
  dash1.y = 490;
  dash1.strokeWeight = 14;
  dash1.strokes = [{ type: 'SOLID', color: { r: 0.14, g: 0.38, b: 0.96 } }];
  dash1.dashPattern = [12, 8];
  frame.appendChild(dash1);

  const dash2 = figma.createLine();
  dash2.resizeWithoutConstraints(780, 0);
  dash2.x = 220;
  dash2.y = 800;
  dash2.strokeWeight = 14;
  dash2.strokes = [{ type: 'SOLID', color: { r: 0.96, g: 0.45, b: 0.14 } }];
  dash2.dashPattern = [20, 6, 4, 6];
  frame.appendChild(dash2);
}

async function buildMaskMatrix(frame: FrameNode): Promise<void> {
  const base = makeRect(frame, 180, 110, 920, 800, { r: 0.95, g: 0.95, b: 0.95 });
  base.cornerRadius = 16;

  const vectorMask = figma.createEllipse();
  vectorMask.resizeWithoutConstraints(280, 280);
  vectorMask.x = 220;
  vectorMask.y = 180;
  vectorMask.fills = [{ type: 'SOLID', color: { r: 1, g: 1, b: 1 } }];
  vectorMask.isMask = true;
  frame.appendChild(vectorMask);
  makeRect(frame, 180, 140, 420, 360, { r: 0.25, g: 0.6, b: 0.95 });

  const textMask = await addLabel(frame, 'MASK', 650, 240, 96);
  textMask.isMask = true;
  makeRect(frame, 620, 180, 400, 220, { r: 0.96, g: 0.4, b: 0.4 });
}

function buildImageFillMatrix(frame: FrameNode): void {
  const pngBytes = new Uint8Array([
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
    8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192,
    240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
  ]);
  const image = figma.createImage(pngBytes);
  const fillBase: ImagePaint = {
    type: 'IMAGE',
    imageHash: image.hash,
    scaleMode: 'FILL',
  };

  const fill = makeRect(frame, 180, 110, 280, 220);
  fill.fills = [fillBase];
  const fit = makeRect(frame, 500, 110, 280, 220);
  fit.fills = [{ ...fillBase, scaleMode: 'FIT' }];
  const crop = makeRect(frame, 820, 110, 280, 220);
  crop.fills = [{ ...fillBase, scaleMode: 'CROP', imageTransform: [[1, 0, 0], [0, 1, 0]] }];
}

async function buildMixedTextRuns(frame: FrameNode): Promise<void> {
  const text = await addLabel(frame, 'Mixed style runs baseline', 200, 180, 28);
  text.characters = 'Regular Bold Italic Color Spacing';
  const baseFont = text.fontName as FontName;
  await figma.loadFontAsync(baseFont);
  text.setRangeFills(0, 7, [{ type: 'SOLID', color: { r: 0.1, g: 0.1, b: 0.1 } }]);
  text.setRangeFills(8, 12, [{ type: 'SOLID', color: { r: 0.95, g: 0.3, b: 0.3 } }]);
  text.setRangeFills(13, 19, [{ type: 'SOLID', color: { r: 0.2, g: 0.45, b: 0.95 } }]);
  text.setRangeLetterSpacing(20, text.characters.length, { unit: 'PIXELS', value: 2 });
}

async function buildAutoLayoutMatrix(frame: FrameNode): Promise<void> {
  const container = figma.createFrame();
  container.x = 180;
  container.y = 120;
  container.resizeWithoutConstraints(980, 760);
  container.layoutMode = 'VERTICAL';
  container.primaryAxisSizingMode = 'AUTO';
  container.counterAxisSizingMode = 'AUTO';
  container.itemSpacing = 24;
  container.paddingTop = 24;
  container.paddingBottom = 24;
  container.paddingLeft = 24;
  container.paddingRight = 24;
  container.fills = [{ type: 'SOLID', color: { r: 1, g: 1, b: 1 } }];
  frame.appendChild(container);

  const row = figma.createFrame();
  row.layoutMode = 'HORIZONTAL';
  row.primaryAxisSizingMode = 'AUTO';
  row.counterAxisSizingMode = 'AUTO';
  row.itemSpacing = 12;
  row.fills = [];
  container.appendChild(row);
  makeRect(row, 0, 0, 140, 64, { r: 0.2, g: 0.45, b: 0.95 });
  makeRect(row, 0, 0, 220, 64, { r: 0.95, g: 0.45, b: 0.2 });

  const fillChild = figma.createFrame();
  fillChild.layoutMode = 'HORIZONTAL';
  fillChild.layoutSizingHorizontal = 'FILL';
  fillChild.primaryAxisSizingMode = 'FIXED';
  fillChild.resizeWithoutConstraints(640, 120);
  fillChild.fills = [{ type: 'SOLID', color: { r: 0.93, g: 0.96, b: 1 } }];
  container.appendChild(fillChild);
}

async function buildKitchenSink(frame: FrameNode): Promise<void> {
  buildGradientMatrix(frame);
  buildBlendModeStack(frame);
  buildImageFillMatrix(frame);
  await buildMixedTextRuns(frame);
}

function postSelectionSummary(): void {
  const selection = figma.currentPage.selection;
  figma.ui.postMessage({
    type: 'selection-summary',
    selectionCount: selection.length,
    selectedNames: selection.map((n) => `${n.name} (${n.type})`),
    hasSelection: selection.length > 0,
  });
}

function postPinnedRootsSummary(): void {
  const roots = resolvePinnedRoots();
  figma.ui.postMessage({
    type: 'pinned-roots-summary',
    count: roots.length,
    names: roots.map((n) => `${n.name} (${n.type})`),
    hasPinned: roots.length > 0,
  });
}

function postFrameCatalog(): void {
  const frames = figma.currentPage.findAll((n) => n.type === 'FRAME') as FrameNode[];
  figma.ui.postMessage({
    type: 'frame-catalog',
    frames: frames.map((f) => ({ id: f.id, name: f.name, type: f.type })),
    pinnedIds: [...pinnedExportRootIds],
  });
}

function prunePinnedRoots(): void {
  pinnedExportRootIds = pinnedExportRootIds.filter((id) => {
    const node = figma.getNodeById(id);
    return Boolean(node && node.type === 'FRAME');
  });
}

function maybeAutoPinMainFrame(): void {
  if (hasManualRootSelection || pinnedExportRootIds.length > 0) {
    return;
  }
  const frames = figma.currentPage.findAll((n) => n.type === 'FRAME') as FrameNode[];
  const main = frames.find((f) => f.name.trim().toLowerCase() === 'main');
  if (main) {
    pinnedExportRootIds = [main.id];
  }
}

function rootsFromSelection(): ExportRoot[] {
  const selection = figma.currentPage.selection;
  return selection.filter((n): n is ExportRoot => n.type === 'FRAME');
}

function resolvePinnedRoots(): ExportRoot[] {
  const roots: ExportRoot[] = [];
  for (const id of pinnedExportRootIds) {
    const node = figma.getNodeById(id);
    if (node && node.type === 'FRAME') {
      roots.push(node as ExportRoot);
    }
  }
  return roots;
}

function resolveExportRoots(): ExportRoot[] {
  const pinned = resolvePinnedRoots();
  if (pinned.length > 0) {
    return pinned;
  }
  return rootsFromSelection();
}

function queueExport(): void {
  if (exportQueued) {
    return;
  }

  exportQueued = true;
  setTimeout(async () => {
    exportQueued = false;
    const options = normalizeOptions(undefined);
    // Serialize once, reuse for both UI + bridge
    const result = await buildExportPayload(options);
    if (!result) {
      return;
    }
    const { payload, totalNodes } = result;
    const json = JSON.stringify(payload);
    figma.ui.postMessage({
      type: 'export-result',
      reason: 'live',
      json,
      stats: {
        totalRoots: payload.nodes.length,
        totalNodes,
        bytes: json.length,
      },
    });
    if (bridgeLiveEnabled) {
      await sendToBridge(json, totalNodes, bridgeLiveUrl, true);
    }
  }, 150);
}

function normalizeOptions(input?: Partial<ExportOptions>): ExportOptions {
  return {
    includeSvg: input?.includeSvg ?? true,
    includePng: input?.includePng ?? false,
    maxDepth: Math.max(1, Math.min(32, input?.maxDepth ?? 16)),
  };
}

async function buildExportPayload(
  options: ExportOptions,
): Promise<{ payload: ExportPayload; totalNodes: number } | null> {
  return buildExportPayloadForRoots(resolveExportRoots(), options);
}

async function buildExportPayloadForRoots(
  roots: ExportRoot[],
  options: ExportOptions,
): Promise<{ payload: ExportPayload; totalNodes: number } | null> {
  if (roots.length === 0) {
    return null;
  }

  const assetAccumulator = createAssetAccumulator();
  const nodes = await Promise.all(
    roots.map((node) => serializeNode(node, 0, options, assetAccumulator)),
  );
  const totalNodes = nodes.reduce((count, node) => count + countNodes(node), 0);

  const payload: ExportPayload = {
    schema: 'fts.figma.export/v2',
    generatedAt: new Date().toISOString(),
    source: {
      plugin: 'fts-figma-import',
      pluginVersion: '0.1.0',
      editorType: figma.editorType,
      fileKey: figma.fileKey ?? undefined,
      page: {
        id: figma.currentPage.id,
        name: figma.currentPage.name,
      },
    },
    selection: {
      ids: roots.map((node) => node.id),
      names: roots.map((node) => node.name),
      totalRoots: roots.length,
      totalNodes,
    },
    options,
    assets: assetAccumulator.assets,
    nodes,
  };

  return { payload, totalNodes };
}

function getCore10FrameStatus(): Core10FrameStatus {
  const frames = figma.currentPage.findAll((n) => n.type === 'FRAME') as FrameNode[];
  const frameNameSet = new Set(frames.map((frame) => frame.name.trim()));
  const required = [...CORE10_FRAME_NAMES];
  const found = required.filter((name) => frameNameSet.has(name));
  const missing = required.filter((name) => !frameNameSet.has(name));
  const extraParityFrames = frames
    .map((frame) => frame.name.trim())
    .filter((name) => name.startsWith('parity/') && !required.includes(name as typeof CORE10_FRAME_NAMES[number]))
    .sort((a, b) => a.localeCompare(b));

  return {
    required,
    found,
    missing,
    extraParityFrames,
  };
}

async function exportSelection(options: ExportOptions, reason: 'manual' | 'live'): Promise<void> {
  try {
    const result = await buildExportPayload(options);
    if (!result) {
      figma.ui.postMessage({
        type: 'export-error',
        message:
          'No export roots. Pin a frame with "Select Frame" or select a frame on canvas.',
      });
      return;
    }

    const { payload, totalNodes } = result;
    // Pretty-print for manual (human-readable), compact for live (speed)
    const json = reason === 'manual'
      ? JSON.stringify(payload, null, 2)
      : JSON.stringify(payload);
    figma.ui.postMessage({
      type: 'export-result',
      reason,
      json,
      stats: {
        totalRoots: payload.nodes.length,
        totalNodes,
        bytes: json.length,
      },
    });

    if (reason === 'manual') {
      figma.notify(`Exported ${totalNodes} nodes from ${payload.nodes.length} root(s).`);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    figma.ui.postMessage({
      type: 'export-error',
      message,
    });
  }
}

async function exportToBridge(
  options: ExportOptions,
  bridgeUrl?: string,
  silent = false,
): Promise<void> {
  try {
    const result = await buildExportPayload(options);
    if (!result) {
      figma.ui.postMessage({
        type: 'bridge-error',
        message:
          'No export roots. Pin a frame with "Select Frame" or select a frame on canvas.',
      });
      return;
    }

    const { payload, totalNodes } = result;
    const json = JSON.stringify(payload);
    await sendToBridge(json, totalNodes, bridgeUrl, silent);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    figma.ui.postMessage({
      type: 'bridge-error',
      message,
    });
  }
}

async function exportCore10ToBridge(
  options: ExportOptions,
  bridgeUrl?: string,
  parityRootDir?: string,
): Promise<void> {
  try {
    const frames = figma.currentPage.findAll((n) => n.type === 'FRAME') as FrameNode[];
    const byName = new Map<string, FrameNode>();
    for (const frame of frames) {
      byName.set(frame.name.trim(), frame);
    }

    const missing = CORE10_FRAME_NAMES.filter((name) => !byName.has(name));
    const available = CORE10_FRAME_NAMES.filter((name) => byName.has(name));
    if (available.length === 0) {
      figma.ui.postMessage({
        type: 'bridge-error',
        message: 'No Core frames found on this page. Create at least one Core frame before exporting.',
      });
      return;
    }

    const scenarios: Array<{
      id: string;
      frameName: string;
      exportJson: ExportPayload;
      expectedPngBase64: string;
    }> = [];

    for (const frameName of available) {
      const frame = byName.get(frameName);
      if (!frame) {
        continue;
      }
      const result = await buildExportPayloadForRoots([frame], options);
      if (!result) {
        continue;
      }

      const pngBytes = await frame.exportAsync({
        format: 'PNG',
        constraint: { type: 'SCALE', value: 1 },
      });
      scenarios.push({
        id: frameName.replace(/^parity\//, '').replace(/\//g, '.'),
        frameName,
        exportJson: result.payload,
        expectedPngBase64: bytesToBase64(pngBytes),
      });
    }

    const totalNodes = scenarios.reduce(
      (sum, scenario) => sum + scenario.exportJson.selection.totalNodes,
      0,
    );

    const json = JSON.stringify({
      schema: 'fts.figma.parity-pack/v1',
      generatedAt: new Date().toISOString(),
      source: {
        plugin: 'fts-figma-import',
        pluginVersion: '0.1.0',
        editorType: figma.editorType,
        fileKey: figma.fileKey ?? undefined,
        page: {
          id: figma.currentPage.id,
          name: figma.currentPage.name,
        },
      },
      parityRootDir: parityRootDir?.trim() || undefined,
      scenarios,
    });

    await sendParityPackToBridge(json, totalNodes, bridgeUrl, scenarios.length, missing, 'Core parity pack');
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    figma.ui.postMessage({
      type: 'bridge-error',
      message,
    });
  }
}

async function exportAllParityToBridge(
  options: ExportOptions,
  bridgeUrl?: string,
  parityRootDir?: string,
): Promise<void> {
  try {
    const frames = figma.currentPage.findAll((n) => n.type === 'FRAME') as FrameNode[];
    const parityFrames = frames
      .filter((frame) => frame.name.trim().startsWith('parity/'))
      .sort((a, b) => a.name.localeCompare(b.name));

    if (parityFrames.length === 0) {
      figma.ui.postMessage({
        type: 'bridge-error',
        message: 'No parity frames found on this page. Expected frame names starting with parity/.',
      });
      return;
    }

    const scenarios: Array<{
      id: string;
      frameName: string;
      exportJson: ExportPayload;
      expectedPngBase64: string;
    }> = [];

    for (const frame of parityFrames) {
      const frameName = frame.name.trim();
      const result = await buildExportPayloadForRoots([frame], options);
      if (!result) {
        continue;
      }

      const pngBytes = await frame.exportAsync({
        format: 'PNG',
        constraint: { type: 'SCALE', value: 1 },
      });
      scenarios.push({
        id: frameName.replace(/^parity\//, '').replace(/\//g, '.'),
        frameName,
        exportJson: result.payload,
        expectedPngBase64: bytesToBase64(pngBytes),
      });
    }

    const totalNodes = scenarios.reduce(
      (sum, scenario) => sum + scenario.exportJson.selection.totalNodes,
      0,
    );

    const json = JSON.stringify({
      schema: 'fts.figma.parity-pack/v1',
      generatedAt: new Date().toISOString(),
      source: {
        plugin: 'fts-figma-import',
        pluginVersion: '0.1.0',
        editorType: figma.editorType,
        fileKey: figma.fileKey ?? undefined,
        page: {
          id: figma.currentPage.id,
          name: figma.currentPage.name,
        },
      },
      parityRootDir: parityRootDir?.trim() || undefined,
      scenarios,
    });

    await sendParityPackToBridge(json, totalNodes, bridgeUrl, scenarios.length, [], 'All parity frames');
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    figma.ui.postMessage({
      type: 'bridge-error',
      message,
    });
  }
}

async function sendToBridge(
  json: string,
  totalNodes: number,
  bridgeUrl?: string,
  silent = false,
): Promise<void> {
  const url = bridgeUrl?.trim() || 'http://localhost:43123/figma-export';
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: json,
  });

  if (!res.ok) {
    throw new Error(`Bridge responded ${res.status} ${res.statusText}`);
  }

  const responseBody = (await res.json()) as {
    ok?: boolean;
    path?: string;
    bytes?: number;
    error?: string;
  };

  if (!responseBody.ok) {
    throw new Error(responseBody.error ?? 'Bridge failed to write file.');
  }

  figma.ui.postMessage({
    type: 'bridge-result',
    path: responseBody.path,
    bytes: responseBody.bytes,
    totalNodes,
  });
  if (!silent) {
    figma.notify(
      `Sent ${totalNodes} nodes to bridge (${responseBody.path ?? '/tmp/fts-figma-export.json'}).`,
    );
  }
}

async function sendParityPackToBridge(
  json: string,
  totalNodes: number,
  bridgeUrl?: string,
  totalScenarios?: number,
  missingCoreFrames?: readonly string[],
  packLabel = 'Core parity pack',
): Promise<void> {
  const base = bridgeUrl?.trim() || 'http://localhost:43123/figma-export';
  const url = base.endsWith('/figma-export') ? `${base.slice(0, -'/figma-export'.length)}/figma-parity-pack` : `${base.replace(/\/+$/, '')}/figma-parity-pack`;
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: json,
  });

  if (!res.ok) {
    throw new Error(`Bridge responded ${res.status} ${res.statusText}`);
  }

  const responseBody = (await res.json()) as {
    ok?: boolean;
    path?: string;
    bytes?: number;
    error?: string;
    writtenScenarios?: number;
  };

  if (!responseBody.ok) {
    throw new Error(responseBody.error ?? 'Bridge failed to write parity pack.');
  }

  figma.ui.postMessage({
    type: 'bridge-result',
    path: responseBody.path,
    bytes: responseBody.bytes,
    totalNodes,
    totalScenarios: responseBody.writtenScenarios ?? totalScenarios,
    missingCoreFrames: missingCoreFrames ?? [],
  });
  const missingCount = missingCoreFrames?.length ?? 0;
  figma.notify(
    `Exported ${packLabel} (${responseBody.writtenScenarios ?? totalScenarios ?? 0} scenarios${missingCount > 0 ? `, missing ${missingCount}` : ''}).`,
  );
}

async function serializeNode(
  node: SceneNode,
  depth: number,
  options: ExportOptions,
  assets: AssetAccumulator,
  parentBounds?: RectLike,
): Promise<ExportNode> {
  // Early-out for invisible subtrees — still emit a minimal stub so the
  // tree structure is preserved, but skip all expensive work.
  if (!node.visible) {
    return pruneUndefined({
      id: node.id,
      name: node.name,
      type: node.type,
      visible: false,
    } as ExportNode);
  }

  const absoluteBoundingBox = readAbsoluteBoundingBox(node);
  const relativeTransform = normalizeRelativeTransform(
    sanitize(readProp(node, 'relativeTransform')),
    absoluteBoundingBox,
    parentBounds,
  );

  const base: ExportNode = {
    id: node.id,
    name: node.name,
    type: node.type,
    visible: node.visible,
    locked: readProp(node, 'locked'),
    opacity: readProp(node, 'opacity'),
    blendMode: readProp(node, 'blendMode'),
    absoluteBoundingBox,
    relativeTransform,
    size: readSize(node),
    rotation: readProp(node, 'rotation'),
    constraints: sanitize(readProp(node, 'constraints')),
    clipsContent: readProp(node, 'clipsContent'),
    layoutMode: readProp(node, 'layoutMode'),
    layoutWrap: readProp(node, 'layoutWrap'),
    layoutSizingHorizontal: readProp(node, 'layoutSizingHorizontal'),
    layoutSizingVertical: readProp(node, 'layoutSizingVertical'),
    layoutAlign: readProp(node, 'layoutAlign'),
    layoutGrow: readProp(node, 'layoutGrow'),
    primaryAxisSizingMode: readProp(node, 'primaryAxisSizingMode'),
    primaryAxisAlignItems: readProp(node, 'primaryAxisAlignItems'),
    counterAxisSizingMode: readProp(node, 'counterAxisSizingMode'),
    counterAxisAlignItems: readProp(node, 'counterAxisAlignItems'),
    paddingTop: readProp(node, 'paddingTop'),
    paddingRight: readProp(node, 'paddingRight'),
    paddingBottom: readProp(node, 'paddingBottom'),
    paddingLeft: readProp(node, 'paddingLeft'),
    itemSpacing: readProp(node, 'itemSpacing'),
    effects: sanitize(readProp(node, 'effects')),
    fills: sanitize(readProp(node, 'fills')),
    strokes: sanitize(readProp(node, 'strokes')),
    strokeWeight: readProp(node, 'strokeWeight'),
    strokeMiterLimit: readProp(node, 'strokeMiterLimit'),
    strokeAlign: readProp(node, 'strokeAlign'),
    strokeJoin: readProp(node, 'strokeJoin'),
    strokeCap: readProp(node, 'strokeCap'),
    dashPattern: sanitize(readProp(node, 'dashPattern')),
    isMask: readProp(node, 'isMask'),
    maskType: readProp(node, 'maskType'),
    cornerSmoothing: readProp(node, 'cornerSmoothing'),
    cornerRadius: readProp(node, 'cornerRadius'),
    topLeftRadius: readProp(node, 'topLeftRadius'),
    topRightRadius: readProp(node, 'topRightRadius'),
    bottomLeftRadius: readProp(node, 'bottomLeftRadius'),
    bottomRightRadius: readProp(node, 'bottomRightRadius'),
    rectangleCornerRadii: sanitize(readProp(node, 'rectangleCornerRadii')),
    fillGeometry: sanitize(readProp(node, 'fillGeometry')),
    strokeGeometry: sanitize(readProp(node, 'strokeGeometry')),
    componentProperties: sanitize(readProp(node, 'componentProperties')),
    componentPropertyReferences: sanitize(readProp(node, 'componentPropertyReferences')),
    explicitVariableModes: sanitize(readProp(node, 'explicitVariableModes')),
    boundVariables: sanitize(readProp(node, 'boundVariables')),
    pluginData: sanitize(readProp(node, 'pluginData')),
    sharedPluginData: sanitize(readProp(node, 'sharedPluginData')),
    componentId: readProp(node, 'componentId'),
    componentSetId: readProp(node, 'componentSetId'),
    variantProperties: sanitize(readProp(node, 'variantProperties')),
    textStyleId: readProp(node, 'textStyleId'),
  };

  if (node.type === 'TEXT') {
    base.characters = node.characters;
    base.textAlignHorizontal = node.textAlignHorizontal;
    base.textAlignVertical = node.textAlignVertical;
    base.textAutoResize = node.textAutoResize;
    base.lineHeight = sanitize(node.lineHeight);
    base.letterSpacing = sanitize(node.letterSpacing);
    base.paragraphSpacing = node.paragraphSpacing;
    base.paragraphIndent = node.paragraphIndent;
    base.textCase = sanitize(node.textCase) as string;
    base.textDecoration = sanitize(node.textDecoration) as string;

    // Numeric text metrics — resolve mixed values to first-character range
    const fs = node.fontSize;
    if (typeof fs === 'number') {
      base.fontSize = fs;
    } else if (fs === figma.mixed && node.characters.length > 0) {
      base.fontSize = node.getRangeFontSize(0, 1) as number;
    }

    const fn_ = node.fontName;
    if (fn_ && fn_ !== figma.mixed) {
      base.fontFamily = (fn_ as FontName).family;
      base.fontWeight = node.fontWeight as number;
    } else if (fn_ === figma.mixed && node.characters.length > 0) {
      const rangeFn = node.getRangeFontName(0, 1) as FontName;
      base.fontFamily = rangeFn.family;
      base.fontWeight = node.getRangeFontWeight(0, 1) as number;
    }

    // Compute lineHeightPx from the lineHeight object
    const lh = node.lineHeight;
    if (lh && lh !== figma.mixed) {
      const lhObj = lh as LineHeight;
      if (lhObj.unit === 'PIXELS') {
        base.lineHeightPx = lhObj.value;
      } else if (lhObj.unit === 'PERCENT' && base.fontSize) {
        base.lineHeightPx = base.fontSize * lhObj.value / 100;
      }
      // unit === 'AUTO' → leave undefined, renderer computes default
    }

    // Compute letterSpacingPx
    const ls = node.letterSpacing;
    if (ls && ls !== figma.mixed) {
      const lsObj = ls as LetterSpacing;
      if (lsObj.unit === 'PIXELS') {
        base.letterSpacingPx = lsObj.value;
      } else if (lsObj.unit === 'PERCENT' && base.fontSize) {
        base.letterSpacingPx = base.fontSize * lsObj.value / 100;
      }
    }
  }

  if (options.includeSvg || options.includePng) {
    // Only export SVG/PNG for nodes that benefit from rasterized fallback.
    // Frames, groups, and rectangles are rendered natively; exporting them
    // is the dominant cost for complex designs.
    if (needsAssetExport(node)) {
      base.assetRefs = await exportNodeAssets(node, options, assets);
    }
  }

  const imageFillAssetRefs = await exportImageFillAssets(node, assets);
  if (Object.keys(imageFillAssetRefs).length > 0) {
    base.imageFillAssetRefs = imageFillAssetRefs;
  }

  if ('children' in node && depth < options.maxDepth) {
    const sceneChildren = node.children.filter(isSceneNode);
    base.children = await Promise.all(
      sceneChildren.map((child) =>
        serializeNode(child, depth + 1, options, assets, absoluteBoundingBox),
      ),
    );
  }

  return pruneUndefined(base);
}

function normalizeRelativeTransform(
  transform: unknown,
  nodeBounds?: RectLike,
  parentBounds?: RectLike,
): number[][] | undefined {
  if (!transform || !Array.isArray(transform)) {
    return undefined;
  }
  if (!nodeBounds || !parentBounds) {
    return sanitize(transform) as number[][] | undefined;
  }
  if (transform.length < 2) {
    return sanitize(transform) as number[][] | undefined;
  }
  const row0 = transform[0];
  const row1 = transform[1];
  if (!Array.isArray(row0) || !Array.isArray(row1) || row0.length < 3 || row1.length < 3) {
    return sanitize(transform) as number[][] | undefined;
  }

  const localX = nodeBounds.x - parentBounds.x;
  const localY = nodeBounds.y - parentBounds.y;
  const next0 = [...row0];
  const next1 = [...row1];
  next0[2] = localX;
  next1[2] = localY;
  return [next0 as number[], next1 as number[]];
}

async function exportNodeAssets(
  node: SceneNode,
  options: ExportOptions,
  assets: AssetAccumulator,
): Promise<{ svg?: string; png?: string }> {
  const refs: { svg?: string; png?: string } = {};

  if (options.includeSvg && supportsVectorExport(node)) {
    try {
      const svgBytes = await node.exportAsync({ format: 'SVG' });
      const svgBase64 = bytesToBase64(svgBytes);
      refs.svg = recordAsset(assets, 'svg', 'image/svg+xml', svgBase64, svgBytes.length);
    } catch {
      // Ignore unsupported SVG exports for individual nodes.
    }
  }

  if (options.includePng && supportsRasterExport(node)) {
    try {
      const pngBytes = await node.exportAsync({
        format: 'PNG',
        constraint: { type: 'SCALE', value: 1 },
      });
      const pngBase64 = bytesToBase64(pngBytes);
      refs.png = recordAsset(assets, 'png', 'image/png', pngBase64, pngBytes.length);
    } catch {
      // Ignore unsupported PNG exports for individual nodes.
    }
  }

  return refs;
}

async function exportImageFillAssets(
  node: SceneNode,
  assets: AssetAccumulator,
): Promise<Record<string, string>> {
  const refs: Record<string, string> = {};
  const fills = readProp<unknown[]>(node, 'fills');
  if (!Array.isArray(fills)) {
    return refs;
  }

  for (const fill of fills) {
    if (!fill || typeof fill !== 'object') {
      continue;
    }
    const paint = fill as { type?: unknown; imageHash?: unknown };
    if (paint.type !== 'IMAGE' || typeof paint.imageHash !== 'string' || !paint.imageHash) {
      continue;
    }

    // Use Figma's imageHash as the dedup key — avoids hashing multi-MB base64 strings
    const dedupKey = `image-fill:${paint.imageHash}`;
    const existingId = assets.byHash.get(dedupKey);
    if (existingId) {
      refs[paint.imageHash] = existingId;
      continue;
    }

    const image = figma.getImageByHash(paint.imageHash);
    if (!image) {
      continue;
    }
    try {
      const bytes = await image.getBytesAsync();
      const base64 = bytesToBase64(bytes);
      const id = `image-fill:${paint.imageHash}`;
      assets.assets.push({
        id,
        kind: 'image-fill',
        mime: 'application/octet-stream',
        encoding: 'base64',
        data: base64,
        byteLength: bytes.length,
      });
      assets.byHash.set(dedupKey, id);
      refs[paint.imageHash] = id;
    } catch {
      // Ignore image decode/export failures for individual fills.
    }
  }

  return refs;
}

function needsAssetExport(node: SceneNode): boolean {
  // These types have complex geometry that our renderer handles natively
  // via fill/stroke/layout — no SVG fallback needed.
  switch (node.type) {
    case 'FRAME':
    case 'GROUP':
    case 'RECTANGLE':
    case 'TEXT':
    case 'SECTION':
    case 'SLICE':
      return false;
    default:
      // Component instances, vectors, boolean ops, ellipses, polygons,
      // stars, lines — benefit from SVG fallback.
      return true;
  }
}

function supportsVectorExport(node: SceneNode): boolean {
  return node.type !== 'SLICE';
}

function supportsRasterExport(node: SceneNode): boolean {
  return node.type !== 'SLICE';
}

function isSceneNode(node: BaseNode): node is SceneNode {
  return (
    node.type !== 'PAGE' &&
    node.type !== 'DOCUMENT' &&
    node.type !== 'SECTION' &&
    node.type !== 'STICKY' &&
    node.type !== 'SHAPE_WITH_TEXT' &&
    node.type !== 'CONNECTOR' &&
    node.type !== 'WASHI_TAPE' &&
    node.type !== 'HIGHLIGHT'
  );
}

function readAbsoluteBoundingBox(node: SceneNode): RectLike | undefined {
  const bounds = readProp<RectLike>(node, 'absoluteRenderBounds') ?? readProp<RectLike>(node, 'absoluteBoundingBox');
  if (!bounds) {
    return undefined;
  }
  return {
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
  };
}

function readSize(node: SceneNode): SizeLike | undefined {
  const width = readProp<number>(node, 'width');
  const height = readProp<number>(node, 'height');
  if (typeof width !== 'number' || typeof height !== 'number') {
    return undefined;
  }
  return { x: width, y: height };
}

function readProp<T>(node: unknown, key: string): T | undefined {
  const value = (node as Record<string, unknown>)[key];
  return value as T | undefined;
}

function sanitize<T>(value: T): T {
  if (value === undefined || value === null) {
    return value;
  }

  if (value === figma.mixed) {
    return '__MIXED__' as T;
  }

  // Primitives do not need deep JSON sanitization.
  if (typeof value !== 'object') {
    return value;
  }

  // Fast path: arrays of numbers/strings (e.g., dashPattern, rectangleCornerRadii,
  // relativeTransform rows) — these never contain figma.mixed or bigint.
  if (Array.isArray(value) && value.length > 0) {
    const first = value[0];
    if (typeof first === 'number' || typeof first === 'string') {
      return value as T;
    }
    // Array of arrays of numbers (e.g., relativeTransform) — also safe
    if (Array.isArray(first) && first.length > 0 && typeof first[0] === 'number') {
      return value as T;
    }
  }

  const json = JSON.stringify(value, (_k, v) => {
    if (typeof v === 'bigint') {
      return v.toString();
    }
    if (typeof v === 'symbol') {
      return String(v);
    }
    if (v === figma.mixed) {
      return '__MIXED__';
    }
    return v;
  });

  if (json === undefined) {
    return value;
  }

  return JSON.parse(json) as T;
}

function pruneUndefined<T extends Record<string, unknown>>(input: T): T {
  const output: Record<string, unknown> = {};
  for (const key of Object.keys(input)) {
    const value = input[key];
    if (value === undefined) {
      continue;
    }
    if (Array.isArray(value) && value.length === 0) {
      continue;
    }
    output[key] = value;
  }
  return output as T;
}

function countNodes(node: ExportNode): number {
  let count = 1;
  for (const child of node.children ?? []) {
    count += countNodes(child);
  }
  return count;
}

function createAssetAccumulator(): AssetAccumulator {
  return {
    assets: [],
    byHash: new Map<string, string>(),
  };
}

function recordAsset(
  assets: AssetAccumulator,
  kind: ExportAssetKind,
  mime: string,
  data: string,
  byteLength: number,
): string {
  const hash = hashString(`${kind}|${mime}|${data}`);
  const existing = assets.byHash.get(hash);
  if (existing) {
    return existing;
  }
  const id = `${kind}:${hash}`;
  assets.assets.push({
    id,
    kind,
    mime,
    encoding: 'base64',
    data,
    byteLength,
  });
  assets.byHash.set(hash, id);
  return id;
}

function hashString(input: string): string {
  // FNV-1a 32-bit (hex) for deterministic asset IDs.
  let hash = 0x811c9dc5;
  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

function bytesToBase64(bytes: Uint8Array): string {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  // Pre-allocate array and join at end — avoids O(n) intermediate string allocations
  const chunks: string[] = [];
  const len = bytes.length;
  for (let i = 0; i < len; i += 3) {
    const a = bytes[i];
    const b = i + 1 < len ? bytes[i + 1] : 0;
    const c = i + 2 < len ? bytes[i + 2] : 0;

    const triple = (a << 16) | (b << 8) | c;
    chunks.push(
      alphabet[(triple >> 18) & 63],
      alphabet[(triple >> 12) & 63],
      i + 1 < len ? alphabet[(triple >> 6) & 63] : '=',
      i + 2 < len ? alphabet[triple & 63] : '=',
    );
  }
  return chunks.join('');
}
