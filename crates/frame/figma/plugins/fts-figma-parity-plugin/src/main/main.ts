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

const EXTRA_ISOLATED_FRAME_NAMES = [
	'parity/primitives/single-pixel-anchor',
	'parity/text/font-weight-style',
	'parity/text/alignment-truncation',
	'parity/effects/shadow-matrix',
	'parity/effects/drop-shadow-matrix',
	'parity/effects/inner-shadow-matrix',
	'parity/effects/effect-ordering',
	'parity/effects/background-blur-stress',
	'parity/paint/blend-mode-combos',
	'parity/masks/mask-clip-interactions',
	'parity/text/text-advanced',
	'parity/images/image-sampling',
	'parity/transform/transform-precision',
	'parity/masks/clip-content-nesting',
	'parity/transform/nested-transform-stack',
] as const;

const FRAME_NAMES = [...CORE10_FRAME_NAMES, ...EXTRA_ISOLATED_FRAME_NAMES] as const;

const FRAME_WIDTH = 1920;
const FRAME_HEIGHT = 1080;
const COL_GAP = 60;
const ROW_GAP = 60;
const COLUMNS = 2;
const GENERATOR_VERSION = 'v2.1';
const RUNTIME_LABEL = `FTS parity generator ${GENERATOR_VERSION}`;
const DEFAULT_BRIDGE_EXPORT_URL = 'http://localhost:43123/figma-parity-pack';
const DEFAULT_BRIDGE_HEALTH_URL = 'http://localhost:43123/health?source=fts-figma-parity-plugin';
const DEFAULT_PARITY_ROOT_DIR =
	'/Users/codywright/Documents/Development/FastTrackStudio/frame/features/render/tests/parity/scenarios';

export default async function () {
	const page = figma.currentPage;
	figma.notify(`${RUNTIME_LABEL} starting...`);
	const bridgePreflight = await preflightBridge();
	const availableFonts = await figma.listAvailableFontsAsync();
	const existingFrames = page.findAll((n) => n.type === 'FRAME') as FrameNode[];
	const byName = new Map(existingFrames.map((frame) => [frame.name.trim(), frame]));
	const created: FrameNode[] = [];

	for (let i = 0; i < FRAME_NAMES.length; i += 1) {
		const frameName = FRAME_NAMES[i];
		const frame = byName.get(frameName) ?? figma.createFrame();
		if (!frame.parent) {
			page.appendChild(frame);
		}
		frame.name = frameName;
		byName.set(frameName, frame);
		frame.resizeWithoutConstraints(FRAME_WIDTH, FRAME_HEIGHT);
		frame.fills = [{ type: 'SOLID', color: rgb(245, 245, 245) }];
		frame.x = (i % COLUMNS) * (FRAME_WIDTH + COL_GAP);
		frame.y = Math.floor(i / COLUMNS) * (FRAME_HEIGHT + ROW_GAP);

		for (const child of [...frame.children]) {
			child.remove();
		}
		await populateScenario(frame, frameName, availableFonts);
		created.push(frame);
	}

	const exportResult = await exportAllPngsToBridge(byName);

	const marker = figma.createText();
	await figma.loadFontAsync(marker.fontName as FontName);
	marker.characters = `${RUNTIME_LABEL} | frames: ${FRAME_NAMES.length}`;
	marker.fontSize = 20;
	marker.x = 24;
	marker.y = Math.ceil(FRAME_NAMES.length / COLUMNS) * (FRAME_HEIGHT + ROW_GAP) + 24;
	page.appendChild(marker);

	figma.currentPage.selection = created;
	figma.viewport.scrollAndZoomIntoView(created);
	figma.closePlugin(
		`Generated ${created.length}/${FRAME_NAMES.length} frames (${GENERATOR_VERSION}). Bridge preflight: ${bridgePreflight}. Exported ${exportResult.written}/${FRAME_NAMES.length} PNGs${exportResult.error ? ` (${exportResult.error})` : ''}.`,
	);
}

async function preflightBridge(): Promise<'ok' | string> {
	try {
		const res = await fetch(DEFAULT_BRIDGE_HEALTH_URL, { method: 'GET' });
		if (!res.ok) {
			return `health ${res.status}`;
		}
		return 'ok';
	} catch (error) {
		return error instanceof Error ? error.message : String(error);
	}
}

async function exportAllPngsToBridge(byName: Map<string, FrameNode>): Promise<{ written: number; error?: string }> {
	const scenarios: Array<{
		id: string;
		frameName: string;
		exportJson: unknown;
		expectedPngBase64: string;
		expectedSvgBase64: string;
	}> = [];
	const exportErrors: string[] = [];
	for (const frameName of FRAME_NAMES) {
		const frame = byName.get(frameName);
		if (!frame) {
			exportErrors.push(`missing frame: ${frameName}`);
			continue;
		}
		try {
			const pngBytes = await exportNodeAsPng(frame);
			const svgBytes = await exportNodeAsSvg(frame);
			scenarios.push({
				id: frameName.replace(/^parity\//, '').replace(/\//g, '.'),
				frameName,
				exportJson: await buildExportPayloadForFrame(frame),
				expectedPngBase64: bytesToBase64(pngBytes),
				expectedSvgBase64: bytesToBase64(svgBytes),
			});
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			exportErrors.push(`${frameName}: ${message}`);
		}
	}
	figma.notify(`Parity export: prepared ${scenarios.length}/${FRAME_NAMES.length} PNG payload(s).`);
	if (exportErrors.length > 0) {
		figma.notify(`Parity export frame errors: ${exportErrors.slice(0, 2).join(' | ')}`);
	}
	if (scenarios.length === 0) {
		return { written: 0, error: exportErrors[0] ?? 'No exportable Core frames.' };
	}
	const payload = JSON.stringify({
		schema: 'fts.figma.parity-pack/v1',
		generatedAt: new Date().toISOString(),
		source: {
			plugin: 'fts-figma-parity-plugin',
			pluginVersion: GENERATOR_VERSION,
			editorType: figma.editorType,
			fileKey: figma.fileKey ?? undefined,
			page: {
				id: figma.currentPage.id,
				name: figma.currentPage.name,
			},
		},
		parityRootDir: DEFAULT_PARITY_ROOT_DIR,
		scenarios,
	});

	try {
		figma.notify('Parity export: checking bridge health...');
		const health = await fetch(DEFAULT_BRIDGE_HEALTH_URL, { method: 'GET' });
		if (!health.ok) {
			throw new Error(`Bridge health check failed ${health.status} ${health.statusText}`);
		}
		figma.notify('Parity export: bridge health OK, uploading PNG pack...');

		const res = await fetch(DEFAULT_BRIDGE_EXPORT_URL, {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: payload,
		});
		if (!res.ok) {
			throw new Error(`Bridge responded ${res.status} ${res.statusText}`);
		}
		const responseBody = (await res.json()) as {
			ok?: boolean;
			error?: string;
			writtenScenarios?: number;
		};
		if (!responseBody.ok) {
			throw new Error(responseBody.error ?? 'Bridge failed to write parity PNGs.');
		}
		figma.notify(`Parity export: wrote ${responseBody.writtenScenarios ?? scenarios.length} scenario PNG(s).`);
		return { written: responseBody.writtenScenarios ?? scenarios.length };
	} catch (error) {
		const message = error instanceof Error ? error.message : String(error);
		figma.notify(`Parity export failed: ${message}`);
		return { written: 0, error: message };
	}
}

type ExportNode = {
	id: string;
	name: string;
	type: string;
	visible: boolean;
	locked?: boolean;
	absoluteBoundingBox?: { x: number; y: number; width: number; height: number };
	relativeTransform?: number[][];
	size?: { x: number; y: number };
	rotation?: number;
	opacity?: number;
	blendMode?: string;
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

type ExportOptions = {
	includeSvg: boolean;
	includePng: boolean;
	maxDepth: number;
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

async function buildExportPayloadForFrame(frame: FrameNode): Promise<ExportPayload> {
	const options: ExportOptions = {
		includeSvg: true,
		includePng: false,
		maxDepth: 16,
	};
	const assetAccumulator = createAssetAccumulator();
	const root = await serializeNode(frame, 0, options, assetAccumulator);
	return {
		schema: 'fts.figma.export/v2',
		generatedAt: new Date().toISOString(),
		source: {
			plugin: 'fts-figma-parity-plugin',
			pluginVersion: GENERATOR_VERSION,
			editorType: figma.editorType,
			fileKey: figma.fileKey ?? undefined,
			page: { id: figma.currentPage.id, name: figma.currentPage.name },
		},
		selection: {
			ids: [frame.id],
			names: [frame.name],
			totalRoots: 1,
			totalNodes: countNodes(root),
		},
		options,
		assets: assetAccumulator.assets,
		nodes: [root],
	};
}

async function serializeNode(
	node: SceneNode,
	depth: number,
	options: ExportOptions,
	assets: AssetAccumulator,
	parentBounds?: RectLike,
): Promise<ExportNode> {
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

		const lh = node.lineHeight;
		if (lh && lh !== figma.mixed) {
			const lhObj = lh as LineHeight;
			if (lhObj.unit === 'PIXELS') {
				base.lineHeightPx = lhObj.value;
			} else if (lhObj.unit === 'PERCENT' && base.fontSize) {
				base.lineHeightPx = (base.fontSize * lhObj.value) / 100;
			}
		}

		const ls = node.letterSpacing;
		if (ls && ls !== figma.mixed) {
			const lsObj = ls as LetterSpacing;
			if (lsObj.unit === 'PIXELS') {
				base.letterSpacingPx = lsObj.value;
			} else if (lsObj.unit === 'PERCENT' && base.fontSize) {
				base.letterSpacingPx = (base.fontSize * lsObj.value) / 100;
			}
		}
	}

	if (options.includeSvg || options.includePng) {
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
	switch (node.type) {
		case 'FRAME':
		case 'GROUP':
		case 'RECTANGLE':
		case 'TEXT':
		case 'SECTION':
		case 'SLICE':
			return false;
		default:
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

	if (typeof value !== 'object') {
		return value;
	}

	if (Array.isArray(value) && value.length > 0) {
		const first = value[0];
		if (typeof first === 'number' || typeof first === 'string') {
			return value as T;
		}
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
	let hash = 0x811c9dc5;
	for (let i = 0; i < input.length; i += 1) {
		hash ^= input.charCodeAt(i);
		hash = Math.imul(hash, 0x01000193) >>> 0;
	}
	return hash.toString(16).padStart(8, '0');
}

async function exportNodeAsPng(node: SceneNode): Promise<Uint8Array> {
	const withContents: ExportSettingsImage = {
		format: 'PNG',
		constraint: { type: 'SCALE', value: 1 },
		contentsOnly: true,
	};
	const plain: ExportSettingsImage = {
		format: 'PNG',
		constraint: { type: 'SCALE', value: 1 },
	};

	const nodeApi = node as unknown as {
		exportAsync?: (s: ExportSettingsImage) => Promise<Uint8Array>;
		exportSettings?: ReadonlyArray<ExportSettings>;
	};
	const figmaApi = figma as unknown as {
		exportAsync?: (n: SceneNode, s: ExportSettingsImage) => Promise<Uint8Array>;
	};

	const withNodeMethod = async (settings: ExportSettingsImage): Promise<Uint8Array | null> => {
		if (typeof nodeApi.exportAsync !== 'function') {
			return null;
		}
		return nodeApi.exportAsync.call(node, settings);
	};

	const withFigmaMethod = async (settings: ExportSettingsImage): Promise<Uint8Array | null> => {
		if (typeof figmaApi.exportAsync !== 'function') {
			return null;
		}
		return figmaApi.exportAsync.call(figma, node, settings);
	};

	try {
		const bytes = await withNodeMethod(withContents);
		if (bytes) {
			return bytes;
		}
	} catch {
		// Fallback to plain settings.
	}
	try {
		const bytes = await withNodeMethod(plain);
		if (bytes) {
			return bytes;
		}
	} catch {
		// Fallback to figma-level export.
	}
	try {
		const bytes = await withFigmaMethod(withContents);
		if (bytes) {
			return bytes;
		}
	} catch {
		// Fallback to plain settings.
	}
	const bytes = await withFigmaMethod(plain);
	if (bytes) {
		return bytes;
	}
	const nodeExportType = typeof nodeApi.exportAsync;
	const figmaExportType = typeof figmaApi.exportAsync;
	const settingsCount = Array.isArray(nodeApi.exportSettings) ? nodeApi.exportSettings.length : 0;
	throw new Error(
		`No compatible export API available (node.exportAsync=${nodeExportType}, figma.exportAsync=${figmaExportType}, exportSettings=${settingsCount}).`,
	);
}

async function exportNodeAsSvg(node: SceneNode): Promise<Uint8Array> {
	const withContents: ExportSettingsSVG = {
		format: 'SVG',
		svgOutlineText: true,
		svgIdAttribute: false,
		svgSimplifyStroke: false,
		contentsOnly: true,
	};
	const plain: ExportSettingsSVG = {
		format: 'SVG',
		svgOutlineText: true,
		svgIdAttribute: false,
		svgSimplifyStroke: false,
	};

	const nodeApi = node as unknown as {
		exportAsync?: (s: ExportSettingsSVG) => Promise<Uint8Array>;
		exportSettings?: ReadonlyArray<ExportSettings>;
	};
	const figmaApi = figma as unknown as {
		exportAsync?: (n: SceneNode, s: ExportSettingsSVG) => Promise<Uint8Array>;
	};

	const withNodeMethod = async (settings: ExportSettingsSVG): Promise<Uint8Array | null> => {
		if (typeof nodeApi.exportAsync !== 'function') {
			return null;
		}
		return nodeApi.exportAsync.call(node, settings);
	};

	const withFigmaMethod = async (settings: ExportSettingsSVG): Promise<Uint8Array | null> => {
		if (typeof figmaApi.exportAsync !== 'function') {
			return null;
		}
		return figmaApi.exportAsync.call(figma, node, settings);
	};

	try {
		const bytes = await withNodeMethod(withContents);
		if (bytes) {
			return bytes;
		}
	} catch {
		// Fallback to plain settings.
	}
	try {
		const bytes = await withNodeMethod(plain);
		if (bytes) {
			return bytes;
		}
	} catch {
		// Fallback to figma-level export.
	}
	try {
		const bytes = await withFigmaMethod(withContents);
		if (bytes) {
			return bytes;
		}
	} catch {
		// Fallback to plain settings.
	}
	const bytes = await withFigmaMethod(plain);
	if (bytes) {
		return bytes;
	}
	const nodeExportType = typeof nodeApi.exportAsync;
	const figmaExportType = typeof figmaApi.exportAsync;
	const settingsCount = Array.isArray(nodeApi.exportSettings) ? nodeApi.exportSettings.length : 0;
	throw new Error(
		`No compatible SVG export API available (node.exportAsync=${nodeExportType}, figma.exportAsync=${figmaExportType}, exportSettings=${settingsCount}).`,
	);
}

function bytesToBase64(bytes: Uint8Array): string {
	const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
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

async function populateScenario(
	frame: FrameNode,
	frameName: string,
	availableFonts: readonly Font[],
): Promise<void> {
	const title = await addLabel(frame, frameName, 24, 24, 20);
	const rowA = await addLabel(frame, 'Row A: baseline', 24, 72);
	const rowB = await addLabel(frame, 'Row B: edge', 24, 384);
	const rowC = await addLabel(frame, 'Row C: stress', 24, 696);
	let skipTrim = false;

	switch (frameName) {
		case 'parity/primitives/fractional-geometry':
			buildFractionalGeometry(frame);
			break;
		case 'parity/paint/gradient-matrix':
			buildGradientMatrix(frame);
			break;
		case 'parity/paint/blend-mode-stack':
			buildBlendModeStack(frame);
			break;
		case 'parity/effects/layer-background-blur':
			buildBlurMatrix(frame);
			break;
		case 'parity/strokes/caps-joins-dashes':
			buildStrokeMatrix(frame);
			break;
		case 'parity/masks/vector-text-image-mask':
			await buildMaskMatrix(frame);
			break;
		case 'parity/images/fill-mode-matrix':
			buildImageFillMatrix(frame);
			break;
		case 'parity/text/mixed-style-runs':
			await buildMixedTextRuns(frame, availableFonts);
			break;
		case 'parity/layout/auto-layout-hug-fill':
			buildAutoLayoutMatrix(frame);
			break;
		case 'parity/regression/kitchen-sink':
			await buildKitchenSink(frame, availableFonts);
			break;
		case 'parity/text/font-weight-style':
			await buildTextWeightStyle(frame, availableFonts);
			break;
		case 'parity/text/alignment-truncation':
			await buildTextAlignment(frame, availableFonts);
			break;
		case 'parity/effects/shadow-matrix':
			buildShadowMatrix(frame);
			break;
		case 'parity/effects/drop-shadow-matrix':
			buildDropShadowMatrix(frame);
			break;
		case 'parity/effects/inner-shadow-matrix':
			buildInnerShadowMatrix(frame);
			break;
		case 'parity/effects/effect-ordering':
			buildEffectOrdering(frame);
			break;
		case 'parity/effects/background-blur-stress':
			buildBackgroundBlurStress(frame);
			break;
		case 'parity/paint/blend-mode-combos':
			buildBlendModeCombos(frame);
			break;
		case 'parity/masks/mask-clip-interactions':
			buildMaskClipInteractions(frame);
			break;
		case 'parity/text/text-advanced':
			await buildTextAdvanced(frame, availableFonts);
			break;
		case 'parity/images/image-sampling':
			buildImageSampling(frame);
			break;
		case 'parity/transform/transform-precision':
			buildTransformPrecision(frame);
			break;
		case 'parity/masks/clip-content-nesting':
			buildClipNesting(frame);
			break;
		case 'parity/transform/nested-transform-stack':
			buildNestedTransform(frame);
			break;
		case 'parity/primitives/single-pixel-anchor':
			buildSinglePixelAnchor(frame);
			title.remove();
			rowA.remove();
			rowB.remove();
			rowC.remove();
			skipTrim = true;
			break;
		default:
			break;
	}

	if (!skipTrim) {
		trimUnusedRowLabels(frame, title, rowA, rowB, rowC);
	}
}

function rgb(r: number, g: number, b: number): RGB {
	return { r: r / 255, g: g / 255, b: b / 255 };
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

async function resolveTextStyles(
	availableFonts: readonly Font[],
): Promise<{
	regular: FontName;
	bold: FontName;
	italic: FontName;
	boldItalic: FontName;
}> {
	const defaultFont = figma.createText().fontName as FontName;
	const family = defaultFont.family;
	const styles = availableFonts
		.map((font) => font.fontName)
		.filter((font) => font.family === family)
		.map((font) => font.style);

	const pick = (names: string[], fallback: FontName): FontName => {
		const hit = names.find((name) => styles.includes(name));
		return hit ? { family, style: hit } : fallback;
	};

	const regular = pick(['Regular', 'Book', 'Normal'], defaultFont);
	const bold = pick(['Bold', 'Semi Bold', 'Medium'], regular);
	const italic = pick(['Italic', 'Oblique'], regular);
	const boldItalic = pick(['Bold Italic', 'Semi Bold Italic', 'Medium Italic'], bold);

	await figma.loadFontAsync(regular);
	await figma.loadFontAsync(bold);
	await figma.loadFontAsync(italic);
	await figma.loadFontAsync(boldItalic);
	return { regular, bold, italic, boldItalic };
}

function makeRect(
	parent: ChildrenMixin,
	x: number,
	y: number,
	w: number,
	h: number,
	color: RGB = rgb(204, 204, 204),
): RectangleNode {
	const rect = figma.createRectangle();
	rect.resizeWithoutConstraints(w, h);
	rect.x = x;
	rect.y = y;
	rect.fills = [{ type: 'SOLID', color }];
	parent.appendChild(rect);
	return rect;
}

function createChildFrame(
	parent: ChildrenMixin,
	name: string,
	x: number,
	y: number,
	w: number,
	h: number,
	fill?: RGB,
): FrameNode {
	const node = figma.createFrame();
	parent.appendChild(node);
	node.name = name;
	node.x = x;
	node.y = y;
	node.resizeWithoutConstraints(w, h);
	node.fills = fill ? [{ type: 'SOLID', color: fill }] : [];
	return node;
}

function trimUnusedRowLabels(
	frame: FrameNode,
	title: TextNode,
	rowA: TextNode,
	rowB: TextNode,
	rowC: TextNode,
): void {
	const isBackgroundLike = (node: SceneNode): boolean => {
		if (!('width' in node && 'height' in node)) {
			return false;
		}
		const coverage = (node.width * node.height) / (FRAME_WIDTH * FRAME_HEIGHT);
		return coverage > 0.25;
	};

	const hasRowContent = (yMin: number, yMax: number): boolean =>
		frame.children.some((node) => {
			if (node.id === title.id || node.id === rowA.id || node.id === rowB.id || node.id === rowC.id) {
				return false;
			}
			if (!('y' in node && 'height' in node)) {
				return false;
			}
			if (isBackgroundLike(node)) {
				return false;
			}
			const centerY = node.y + node.height / 2;
			return centerY >= yMin && centerY <= yMax;
		});

	// Keep Row A only if there is baseline content.
	if (!hasRowContent(96, 340)) {
		rowA.remove();
	}
	if (!hasRowContent(408, 652)) {
		rowB.remove();
	}
	if (!hasRowContent(720, 964)) {
		rowC.remove();
	}
}

function buildFractionalGeometry(frame: FrameNode): void {
	const rect = makeRect(frame, 180.5, 120.5, 160.5, 80.5, rgb(56, 120, 245));
	rect.strokes = [{ type: 'SOLID', color: rgb(21, 41, 82) }];
	rect.strokeWeight = 1;

	const ellipse = figma.createEllipse();
	ellipse.resizeWithoutConstraints(120.5, 120.5);
	ellipse.x = 390.5;
	ellipse.y = 110.5;
	ellipse.fills = [{ type: 'SOLID', color: rgb(245, 115, 56) }];
	frame.appendChild(ellipse);

	const line = figma.createLine();
	line.resizeWithoutConstraints(220.5, 0);
	line.x = 560.5;
	line.y = 165.5;
	line.strokes = [{ type: 'SOLID', color: rgb(28, 28, 28) }];
	line.strokeWeight = 1;
	frame.appendChild(line);

	makeRect(frame, 180.5, 430.25, 120.25, 120.25, rgb(43, 186, 140));
	makeRect(frame, 315.75, 430.75, 120.25, 120.25, rgb(237, 201, 69));
	makeRect(frame, 451.5, 431.1, 120.25, 120.25, rgb(232, 79, 79));
}

function buildSinglePixelAnchor(frame: FrameNode): void {
	const pixel = makeRect(frame, 137, 211, 1, 1, rgb(0, 0, 0));
	pixel.strokes = [];
}

function buildGradientMatrix(frame: FrameNode): void {
	const linear = makeRect(frame, 180, 110, 280, 180);
	linear.fills = [
		{
			type: 'GRADIENT_LINEAR',
			gradientStops: [
				{ position: 0, color: { ...rgb(255, 60, 60), a: 1 } },
				{ position: 1, color: { ...rgb(60, 60, 255), a: 1 } },
			],
			gradientTransform: [
				[1, 0, 0],
				[0, 1, 0],
			],
		},
	];

	const radial = makeRect(frame, 500, 110, 280, 180);
	radial.fills = [
		{
			type: 'GRADIENT_RADIAL',
			gradientStops: [
				{ position: 0, color: { ...rgb(255, 245, 80), a: 1 } },
				{ position: 1, color: { ...rgb(40, 40, 40), a: 1 } },
			],
			gradientTransform: [
				[1, 0, 0],
				[0, 1, 0],
			],
		},
	];

	const angular = makeRect(frame, 820, 110, 280, 180);
	angular.fills = [
		{
			type: 'GRADIENT_ANGULAR',
			gradientStops: [
				{ position: 0, color: { ...rgb(60, 200, 120), a: 1 } },
				{ position: 0.5, color: { ...rgb(230, 95, 210), a: 1 } },
				{ position: 1, color: { ...rgb(60, 200, 120), a: 1 } },
			],
			gradientTransform: [
				[1, 0, 0],
				[0, 1, 0],
			],
		},
	];
}

function buildBlendModeStack(frame: FrameNode): void {
	const bg = makeRect(frame, 180, 110, 920, 800, rgb(242, 242, 250));
	bg.cornerRadius = 20;

	const r1 = makeRect(frame, 280, 180, 300, 220, rgb(255, 60, 60));
	r1.opacity = 0.75;
	const r2 = makeRect(frame, 420, 250, 300, 220, rgb(60, 153, 255));
	r2.opacity = 0.75;
	r2.blendMode = 'MULTIPLY';
	const r3 = makeRect(frame, 560, 320, 300, 220, rgb(77, 255, 102));
	r3.opacity = 0.75;
	r3.blendMode = 'SCREEN';
}

function buildBlurMatrix(frame: FrameNode): void {
	makeRect(frame, 180, 110, 920, 800, rgb(224, 230, 237));
	const a = makeRect(frame, 240, 160, 220, 140, rgb(56, 120, 245));
	a.effects = [{ type: 'LAYER_BLUR', radius: 2, visible: true }];
	const b = makeRect(frame, 510, 160, 220, 140, rgb(56, 120, 245));
	b.effects = [{ type: 'LAYER_BLUR', radius: 8, visible: true }];
	const c = makeRect(frame, 780, 160, 220, 140, rgb(56, 120, 245));
	c.effects = [{ type: 'LAYER_BLUR', radius: 24, visible: true }];

	const glass = makeRect(frame, 320, 430, 640, 360, rgb(255, 255, 255));
	glass.opacity = 0.2;
	glass.cornerRadius = 24;
	glass.effects = [{ type: 'BACKGROUND_BLUR', radius: 18, visible: true }];
}

function makeOpenVPath(parent: ChildrenMixin, x: number, y: number): VectorNode {
	const v = figma.createVector();
	v.vectorPaths = [{ windingRule: 'NONZERO', data: 'M 0 120 L 80 0 L 160 120' }];
	v.x = x;
	v.y = y;
	v.fills = [];
	v.strokes = [{ type: 'SOLID', color: rgb(21, 21, 21) }];
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
	dash1.strokes = [{ type: 'SOLID', color: rgb(36, 97, 245) }];
	dash1.dashPattern = [12, 8];
	frame.appendChild(dash1);

	const dash2 = figma.createLine();
	dash2.resizeWithoutConstraints(780, 0);
	dash2.x = 220;
	dash2.y = 800;
	dash2.strokeWeight = 14;
	dash2.strokes = [{ type: 'SOLID', color: rgb(245, 115, 36) }];
	dash2.dashPattern = [20, 6, 4, 6];
	frame.appendChild(dash2);
}

async function buildMaskMatrix(frame: FrameNode): Promise<void> {
	const base = makeRect(frame, 180, 110, 920, 800, rgb(242, 242, 242));
	base.cornerRadius = 16;

	const vectorMask = figma.createEllipse();
	vectorMask.resizeWithoutConstraints(280, 280);
	vectorMask.x = 220;
	vectorMask.y = 180;
	vectorMask.fills = [{ type: 'SOLID', color: rgb(255, 255, 255) }];
	vectorMask.isMask = true;
	frame.appendChild(vectorMask);
	makeRect(frame, 180, 140, 420, 360, rgb(64, 153, 242));

	const textMask = await addLabel(frame, 'MASK', 650, 240, 96);
	textMask.isMask = true;
	makeRect(frame, 620, 180, 400, 220, rgb(245, 102, 102));
}

function buildImageFillMatrix(frame: FrameNode): void {
	const pngBytes = new Uint8Array([
		137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8,
		6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
		31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
	]);
	const image = figma.createImage(pngBytes);
	const fill: ImagePaint = {
		type: 'IMAGE',
		imageHash: image.hash,
		scaleMode: 'FILL',
	};

	const a = makeRect(frame, 180, 110, 280, 220);
	a.fills = [fill];
	const b = makeRect(frame, 500, 110, 280, 220);
	b.fills = [{ ...fill, scaleMode: 'FIT' }];
	const c = makeRect(frame, 820, 110, 280, 220);
	c.fills = [
		{
			...fill,
			scaleMode: 'CROP',
			imageTransform: [
				[1, 0, 0],
				[0, 1, 0],
			],
		},
	];
}

async function buildMixedTextRuns(frame: FrameNode, availableFonts: readonly Font[]): Promise<void> {
	const styles = await resolveTextStyles(availableFonts);
	const textA = await addLabel(
		frame,
		'Regular Bold Italic BoldItalic Color Spacing',
		200,
		180,
		28,
	);
	textA.fontName = styles.regular;
	textA.setRangeFontName(8, 12, styles.bold);
	textA.setRangeFontName(13, 19, styles.italic);
	textA.setRangeFontName(20, 30, styles.boldItalic);
	textA.setRangeFills(0, 7, [{ type: 'SOLID', color: rgb(28, 28, 28) }]);
	textA.setRangeFills(31, 36, [{ type: 'SOLID', color: rgb(245, 80, 80) }]);
	textA.setRangeFills(37, textA.characters.length, [{ type: 'SOLID', color: rgb(60, 115, 245) }]);
	textA.setRangeLetterSpacing(37, textA.characters.length, { unit: 'PIXELS', value: 2 });

	const textB = figma.createText();
	textB.fontName = styles.regular;
	textB.fontSize = 24;
	textB.characters = 'Row B mixed styles with wrap: Bold segment, italic segment, and colored segment in one paragraph to test run boundaries.';
	textB.resize(1120, 120);
	textB.x = 200;
	textB.y = 470;
	frame.appendChild(textB);
	textB.setRangeFontName(32, 44, styles.bold);
	textB.setRangeFontName(46, 60, styles.italic);
	textB.setRangeFills(66, 80, [{ type: 'SOLID', color: rgb(70, 145, 245) }]);
	textB.setRangeLetterSpacing(81, textB.characters.length, { unit: 'PIXELS', value: 0.8 });

	const textC = figma.createText();
	textC.fontName = styles.regular;
	textC.fontSize = 22;
	textC.characters = 'Row C stress: multi-line\nBold + Italic + BoldItalic\nwith spacing and color shifts';
	textC.resize(780, 170);
	textC.x = 200;
	textC.y = 780;
	frame.appendChild(textC);
	textC.setRangeFontName(25, 29, styles.bold);
	textC.setRangeFontName(32, 38, styles.italic);
	textC.setRangeFontName(41, 51, styles.boldItalic);
	textC.setRangeFills(57, textC.characters.length, [{ type: 'SOLID', color: rgb(245, 120, 80) }]);
	textC.setRangeLetterSpacing(57, textC.characters.length, { unit: 'PIXELS', value: 1.4 });
}

function buildAutoLayoutMatrix(frame: FrameNode): void {
	const container = createChildFrame(
		frame,
		'__auto-layout-container',
		180,
		120,
		980,
		760,
		rgb(255, 255, 255),
	);
	container.layoutMode = 'VERTICAL';
	container.primaryAxisSizingMode = 'AUTO';
	container.counterAxisSizingMode = 'AUTO';
	container.itemSpacing = 24;
	container.paddingTop = 24;
	container.paddingBottom = 24;
	container.paddingLeft = 24;
	container.paddingRight = 24;

	const row = createChildFrame(container, '__auto-layout-row', 0, 0, 0, 0);
	row.layoutMode = 'HORIZONTAL';
	row.primaryAxisSizingMode = 'AUTO';
	row.counterAxisSizingMode = 'AUTO';
	row.itemSpacing = 12;
	makeRect(row, 0, 0, 140, 64, rgb(56, 120, 245));
	makeRect(row, 0, 0, 220, 64, rgb(245, 120, 56));

	const fillChild = createChildFrame(container, '__auto-layout-fill-child', 0, 0, 640, 120, rgb(237, 245, 255));
	fillChild.layoutMode = 'HORIZONTAL';
	fillChild.layoutSizingHorizontal = 'FILL';
	fillChild.primaryAxisSizingMode = 'FIXED';
}

async function buildKitchenSink(frame: FrameNode, availableFonts: readonly Font[]): Promise<void> {
	buildGradientMatrix(frame);
	buildBlendModeStack(frame);
	buildImageFillMatrix(frame);
	await buildMixedTextRuns(frame, availableFonts);
	buildShadowMatrix(frame);
	buildNestedTransform(frame);
}

async function buildTextWeightStyle(frame: FrameNode, availableFonts: readonly Font[]): Promise<void> {
	const styles = await resolveTextStyles(availableFonts);
	const sample = await addLabel(frame, 'Regular  Bold  Italic  BoldItalic', 180, 150, 42);
	sample.fontName = styles.regular;
	sample.setRangeFontName(9, 13, styles.bold);
	sample.setRangeFontName(15, 21, styles.italic);
	sample.setRangeFontName(23, sample.characters.length, styles.boldItalic);
}

async function buildTextAlignment(frame: FrameNode, availableFonts: readonly Font[]): Promise<void> {
	const styles = await resolveTextStyles(availableFonts);
	for (let i = 0; i < 3; i += 1) {
		const box = makeRect(frame, 180 + i * 320, 150, 280, 180, rgb(255, 255, 255));
		box.strokes = [{ type: 'SOLID', color: rgb(210, 210, 210) }];
		const text = figma.createText();
		text.fontName = styles.regular;
		text.characters = 'Alignment sample text that wraps.';
		text.resize(260, 160);
		text.x = box.x + 10;
		text.y = box.y + 10;
		text.textAlignHorizontal = i === 0 ? 'LEFT' : i === 1 ? 'CENTER' : 'RIGHT';
		frame.appendChild(text);
	}
}

function buildShadowMatrix(frame: FrameNode): void {
	const a = makeRect(frame, 200, 140, 240, 150, rgb(255, 255, 255));
	a.effects = [
		{
			type: 'DROP_SHADOW',
			color: { ...rgb(20, 20, 20), a: 0.25 },
			offset: { x: 0, y: 6 },
			radius: 8,
			spread: 0,
			visible: true,
			blendMode: 'NORMAL',
		},
	];
	const b = makeRect(frame, 520, 140, 240, 150, rgb(255, 255, 255));
	b.effects = [
		{
			type: 'INNER_SHADOW',
			color: { ...rgb(20, 20, 20), a: 0.35 },
			offset: { x: 0, y: 2 },
			radius: 6,
			spread: 0,
			visible: true,
			blendMode: 'NORMAL',
		},
	];
}

function buildClipNesting(frame: FrameNode): void {
	const outer = createChildFrame(frame, '__clip-outer', 180, 140, 700, 420, rgb(255, 255, 255));
	outer.clipsContent = true;

	const inner = createChildFrame(outer, '__clip-inner', 80, 60, 500, 280, rgb(242, 245, 255));
	inner.clipsContent = true;

	makeRect(inner, -120, 40, 300, 220, rgb(245, 100, 100));
	makeRect(inner, 160, -80, 300, 220, rgb(100, 160, 245));
}

function buildNestedTransform(frame: FrameNode): void {
	const base = createChildFrame(frame, '__nested-transform-base', 220, 160, 560, 420, rgb(255, 255, 255));

	const p = makeRect(base, 120, 80, 260, 180, rgb(80, 130, 245));
	p.rotation = 12;
	const c = makeRect(base, 180, 120, 220, 130, rgb(245, 130, 80));
	c.rotation = -18;
	const g = makeRect(base, 240, 160, 140, 90, rgb(100, 220, 150));
	g.rotation = 22;
}

function buildDropShadowMatrix(frame: FrameNode): void {
	const offsets = [2, 8, 16];
	const blurs = [4, 12, 28];
	for (let i = 0; i < 3; i += 1) {
		const rect = makeRect(frame, 200 + i * 280, 140, 220, 140, rgb(255, 255, 255));
		rect.cornerRadius = 16;
		rect.effects = [
			{
				type: 'DROP_SHADOW',
				color: { ...rgb(24, 24, 24), a: 0.22 + i * 0.1 },
				offset: { x: offsets[i], y: offsets[i] },
				radius: blurs[i],
				spread: i * 2,
				visible: true,
				blendMode: 'NORMAL',
			},
		];
	}
	const stacked = makeRect(frame, 280, 470, 520, 220, rgb(255, 255, 255));
	stacked.cornerRadius = 24;
	stacked.effects = [
		{
			type: 'DROP_SHADOW',
			color: { ...rgb(0, 0, 0), a: 0.22 },
			offset: { x: 0, y: 2 },
			radius: 4,
			spread: 0,
			visible: true,
			blendMode: 'NORMAL',
		},
		{
			type: 'DROP_SHADOW',
			color: { ...rgb(0, 0, 0), a: 0.2 },
			offset: { x: 0, y: 16 },
			radius: 24,
			spread: 0,
			visible: true,
			blendMode: 'NORMAL',
		},
	];
}

function buildInnerShadowMatrix(frame: FrameNode): void {
	for (let i = 0; i < 3; i += 1) {
		const rect = makeRect(frame, 200 + i * 280, 140, 220, 140, rgb(240, 244, 252));
		rect.cornerRadius = 16;
		rect.effects = [
			{
				type: 'INNER_SHADOW',
				color: { ...rgb(25, 25, 30), a: 0.2 + i * 0.12 },
				offset: { x: i * 2, y: i * 2 + 1 },
				radius: 4 + i * 5,
				spread: 0,
				visible: true,
				blendMode: 'NORMAL',
			},
		];
	}
}

function buildEffectOrdering(frame: FrameNode): void {
	const a = makeRect(frame, 220, 150, 260, 180, rgb(76, 132, 245));
	a.strokeWeight = 12;
	a.strokes = [{ type: 'SOLID', color: rgb(20, 40, 80) }];
	a.effects = [
		{
			type: 'DROP_SHADOW',
			color: { ...rgb(0, 0, 0), a: 0.24 },
			offset: { x: 0, y: 12 },
			radius: 20,
			spread: 0,
			visible: true,
			blendMode: 'NORMAL',
		},
	];

	const b = makeRect(frame, 560, 150, 260, 180, rgb(245, 132, 76));
	b.effects = [
		{ type: 'LAYER_BLUR', radius: 10, visible: true },
		{
			type: 'DROP_SHADOW',
			color: { ...rgb(0, 0, 0), a: 0.24 },
			offset: { x: 0, y: 10 },
			radius: 16,
			spread: 0,
			visible: true,
			blendMode: 'NORMAL',
		},
	];
}

function buildBackgroundBlurStress(frame: FrameNode): void {
	for (let i = 0; i < 14; i += 1) {
		const tile = makeRect(
			frame,
			160 + (i % 7) * 140,
			130 + Math.floor(i / 7) * 140,
			120,
			120,
			i % 2 === 0 ? rgb(245, 90, 90) : rgb(90, 140, 245),
		);
		tile.rotation = i % 3 === 0 ? 8 : -6;
	}
	const glassA = makeRect(frame, 240, 420, 340, 220, rgb(255, 255, 255));
	glassA.opacity = 0.18;
	glassA.cornerRadius = 24;
	glassA.effects = [{ type: 'BACKGROUND_BLUR', radius: 12, visible: true }];
	const glassB = makeRect(frame, 620, 420, 340, 220, rgb(255, 255, 255));
	glassB.opacity = 0.18;
	glassB.cornerRadius = 24;
	glassB.effects = [{ type: 'BACKGROUND_BLUR', radius: 24, visible: true }];
}

function buildBlendModeCombos(frame: FrameNode): void {
	const modes: BlendMode[] = ['MULTIPLY', 'SCREEN', 'OVERLAY', 'SOFT_LIGHT'];
	for (let i = 0; i < modes.length; i += 1) {
		const x = 180 + i * 260;
		makeRect(frame, x, 140, 210, 210, rgb(238, 238, 246));
		const b1 = makeRect(frame, x + 24, 164, 110, 110, rgb(245, 80, 80));
		b1.opacity = 0.75;
		const b2 = makeRect(frame, x + 76, 216, 110, 110, rgb(80, 150, 245));
		b2.opacity = 0.75;
		b2.blendMode = modes[i];
	}
}

function buildMaskClipInteractions(frame: FrameNode): void {
	buildClipNesting(frame);
	const mask = figma.createEllipse();
	mask.resizeWithoutConstraints(220, 220);
	mask.x = 980;
	mask.y = 220;
	mask.fills = [{ type: 'SOLID', color: rgb(255, 255, 255) }];
	mask.isMask = true;
	frame.appendChild(mask);
	const colored = makeRect(frame, 920, 180, 360, 280, rgb(94, 171, 245));
	colored.effects = [
		{
			type: 'DROP_SHADOW',
			color: { ...rgb(0, 0, 0), a: 0.25 },
			offset: { x: 0, y: 10 },
			radius: 14,
			spread: 0,
			visible: true,
			blendMode: 'NORMAL',
		},
	];
}

async function buildTextAdvanced(frame: FrameNode, availableFonts: readonly Font[]): Promise<void> {
	const styles = await resolveTextStyles(availableFonts);
	const text = await addLabel(
		frame,
		'Regular  Bold  Italic  BoldItalic\nLineHeight Tight -> Loose',
		180,
		140,
		32,
	);
	text.fontName = styles.regular;
	text.setRangeFontName(9, 13, styles.bold);
	text.setRangeFontName(15, 21, styles.italic);
	text.setRangeFontName(23, 33, styles.boldItalic);
	text.setRangeLineHeight(0, 33, { unit: 'PIXELS', value: 32 });
	text.setRangeLineHeight(34, text.characters.length, { unit: 'PIXELS', value: 56 });
	text.setRangeLetterSpacing(0, 33, { unit: 'PIXELS', value: 1.2 });
}

function buildImageSampling(frame: FrameNode): void {
	const base = new Uint8Array([
		137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8,
		6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 48, 48, 248,
		15, 0, 4, 254, 1, 255, 230, 145, 42, 244, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
	]);
	const image = figma.createImage(base);
	const fill: ImagePaint = { type: 'IMAGE', imageHash: image.hash, scaleMode: 'FILL' };
	const a = makeRect(frame, 180, 140, 220, 220);
	a.fills = [fill];
	const b = makeRect(frame, 450, 140, 500, 120);
	b.fills = [{ ...fill, scaleMode: 'FIT' }];
	const c = makeRect(frame, 1000, 140, 180, 260);
	c.fills = [{ ...fill, scaleMode: 'CROP', imageTransform: [[1, 0, 0], [0, 1, 0]] }];
}

function buildTransformPrecision(frame: FrameNode): void {
	const root = createChildFrame(frame, '__transform-precision-root', 180.5, 140.5, 780, 500, rgb(255, 255, 255));

	const a = makeRect(root, 120.25, 90.5, 320.5, 220.25, rgb(95, 142, 245));
	a.rotation = 11.5;
	const b = makeRect(root, 180.75, 140.25, 240.25, 160.75, rgb(245, 140, 95));
	b.rotation = -17.25;
	const c = makeRect(root, 220.1, 190.3, 180.5, 110.5, rgb(112, 220, 154));
	c.rotation = 22.75;
}
