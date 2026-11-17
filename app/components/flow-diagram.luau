-- A small system diagram on a grid: nodes, links between neighbours, groups
-- around nodes, and tokens that travel the links. The caller names things
-- and places them in grid cells; the component draws them at fractions of
-- its box, so one diagram fits a side panel and a full pane.

type Meter = { text: string, level: number?, tone: string? }
type Node = {
	id: string,
	label: string,
	sublabel: string?,
	icon: string?,
	col: number,
	row: number,
	tone: string?,
	note: string?,
	meter: Meter?,
	planned: boolean?,
}
type Link = { from: string, to: string, label: string? }
type Group = { label: string, nodes: { string } }
type Flow = { path: { string }, tokens: { string }, lane: number? }
-- Where a node sits: the centre of its cell, as fractions of the box.
type Point = { x: number, y: number, col: number, row: number }
type Size = { w: number, h: number, slot: number, icon: number }
type Props = {
	nodes: { Node },
	links: { Link }?,
	groups: { Group }?,
	flows: { Flow }?,
	columns: number?,
	rows: number?,
	time: number?,
	speed: number?,
	selected: string?,
	event: string?,
	to: string?,
	size: string?,
	height: number?,
}

local SIZES: { [string]: Size } = {
	small = { w = 126, h = 56, slot = 20, icon = 22 },
	normal = { w = 136, h = 60, slot = 24, icon = 26 },
}
local TONES = { accent = true, success = true, warning = true, danger = true }

local function tone_of(name: string?): string?
	return if name and TONES[name] then name else nil
end

-- A link under the nodes, with a head at the edge of the node it points at.
-- Only the node's own size is in pixels.
local function link(a: Point, b: Point, size: Size, label: string?)
	if math.abs(a.y - b.y) < 0.001 then
		local right = b.x > a.x
		return ui.row {
			area = { left = math.min(a.x, b.x), top = a.y - 0.08, width = math.abs(b.x - a.x), height = 0.16 },
			align = "center",
			gap = "none",
			ui.row { width = size.w / 2 + 2 },
			not right and ui.icon { name = "chevron-left", size = "small", class = "flow-diagram-head" },
			ui.column {
				grow = true,
				gap = "none",
				ui.row { height = 14, justify = "center", label and ui.text { label, class = "flow-diagram-link-label" } },
				ui.row { class = "flow-diagram-line", height = 2 },
				ui.row { height = 14 },
			},
			right and ui.icon { name = "chevron-right", size = "small", class = "flow-diagram-head" },
			ui.row { width = size.w / 2 + 2 },
		}
	end
	local down = b.y > a.y
	return ui.column {
		area = { left = a.x - 0.05, top = math.min(a.y, b.y), width = 0.1, height = math.abs(b.y - a.y) },
		align = "center",
		gap = "none",
		ui.row { height = size.h / 2 + 2 },
		not down and ui.icon { name = "chevron-up", size = "small", class = "flow-diagram-head" },
		ui.column { class = "flow-diagram-line", grow = true, width = 2 },
		down and ui.icon { name = "chevron-down", size = "small", class = "flow-diagram-head" },
		ui.row { height = size.h / 2 + 2 },
	}
end

-- The tokens of one flow, spread evenly along its path at this time.
local function tokens(flow: Flow, at: { [string]: Point }, seconds: number, speed: number, into: { any })
	local points: { Point }, lens: { number }, total = {}, {}, 0
	for _, id in flow.path do
		if at[id] then
			table.insert(points, at[id])
		end
	end
	for i = 2, #points do
		-- A cell is wider than it is high: weigh the two so the speed looks even.
		local dx, dy = (points[i].x - points[i - 1].x) * 3, (points[i].y - points[i - 1].y) * 1.8
		lens[i - 1] = math.sqrt(dx * dx + dy * dy)
		total += lens[i - 1]
	end
	if total <= 0 or #flow.tokens == 0 then
		return
	end
	for n, tone in flow.tokens do
		local along = (seconds * speed + (n - 1) * total / #flow.tokens) % total
		local i = 1
		while i < #lens and along > lens[i] do
			along -= lens[i]
			i += 1
		end
		local from, to = points[i], points[i + 1]
		local f = along / lens[i]
		local x, y = from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f
		-- Two flows on one link go side by side.
		if math.abs(from.y - to.y) < 0.001 then
			y += (flow.lane or 0) * 0.022
		else
			x += (flow.lane or 0) * 0.016
		end
		table.insert(
			into,
			ui.row {
				area = { left = x - 0.03, top = y - 0.04, width = 0.06, height = 0.08 },
				align = "center",
				justify = "center",
				ui.row { class = { "flow-diagram-token", `flow-diagram-token-{tone_of(tone) or "plain"}` }, width = 10, height = 10 },
			}
		)
	end
end

local function card(node: Node, size: Size, props: Props)
	local tone = tone_of(node.tone)
	local meter = node.meter
	local meter_tone = meter and tone_of(meter.tone) or "plain"
	local classes, icon_classes = { "flow-diagram-node" }, { "flow-diagram-icon" }
	if tone then
		table.insert(classes, `flow-diagram-node-{tone}`)
		table.insert(icon_classes, `flow-diagram-icon-{tone}`)
	end
	if node.planned then
		table.insert(classes, "flow-diagram-node-planned")
	end
	return ui.column {
		class = classes,
		selected = props.selected == node.id,
		width = size.w,
		height = size.h,
		justify = "center",
		gap = "none",
		msg = { [props.event or "select"] = node.id },
		to = props.to,
		tooltip = node.sublabel,
		ui.row {
			class = "flow-diagram-top",
			align = "center",
			ui.row {
				class = icon_classes,
				width = size.icon,
				height = size.icon,
				shrink = false,
				align = "center",
				justify = "center",
				ui.icon { name = (node.icon or "circle") :: any, size = "small" },
			},
			ui.column {
				grow = true,
				gap = "none",
				ui.text { node.label, class = "flow-diagram-label", truncate = true },
				node.sublabel and not meter and ui.text { node.sublabel, class = "flow-diagram-sublabel", truncate = true },
			},
		},
		meter and ui.row {
			class = "flow-diagram-meter",
			align = "center",
			ui.row {
				class = "flow-diagram-track",
				width = 36,
				height = 4,
				shrink = false,
				gap = "none",
				ui.row {
					class = `flow-diagram-fill-{meter_tone}`,
					width = math.max(2, 36 * math.clamp(meter.level or 0, 0, 1)),
					height = 4,
				},
			},
			ui.row { grow = true, ui.text { meter.text, class = { "flow-diagram-meter-text", `flow-diagram-text-{meter_tone}` }, truncate = true } },
		},
	}
end

local EXAMPLE_LINKS: { Link } = {
	{ from = "web", to = "api", label = "HTTPS" },
	{ from = "api", to = "db" },
	{ from = "api", to = "cache" },
}

return component {
	title = "Flow Diagram",
	category = "Data",
	icon = "workflow",
	description = "A small system diagram on a grid: named nodes with an icon and a meter, links between neighbours, a tinted group around nodes, and coloured tokens that travel the links.",
	when = "Explain how the parts of a system talk to each other, and what moves between them now: a request path, a cache miss, a retry, a pipeline. The caller places each node in a grid cell, so the same diagram fits a side panel and a full pane. Pass a growing `time` to move the tokens; leave it out for a still diagram. For a line of commits use `commit-graph`; for a tree use `tree`; for places on a map use `route-map`.",
	props = {
		["nodes"] = "{ id, label, sublabel?, icon?, col, row, tone?, note?, meter? = { text, level?, tone? }, planned? }: `col` and `row` count from 0; `tone` is accent | success | warning | danger",
		["links"] = "{ from, to, label? }: two nodes in one row or one column (a slanted link is left out); the head points at `to`",
		["groups"] = "{ label, nodes = { ids } }: a tinted box around the cells of its nodes",
		["flows"] = "{ path = { ids }, tokens = { tones }, lane? }: one token for each tone, spread along the path; `lane` (-1, 0, 1) keeps two flows on one link apart",
		["columns"] = "the grid's width in cells (default: what the nodes use)",
		["rows"] = "the grid's height in cells (default: what the nodes use)",
		["time"] = "seconds: where the tokens are. The caller owns the clock",
		["speed"] = "how far a token goes each second, in cells (default 0.55)",
		["selected"] = "the id of the node to show as picked",
		["event"] = "key of the message a click on a node sends, default \"select\": { select = id }",
		["to"] = "mod to send the click to, if not the drawing one",
		["size"] = "small | normal: the size of a node",
		["height"] = "pixels (default 120 for each row)",
	},
	styles = {
		[".flow-diagram"] = { background = "muted/40", radius = 10 },
		[".flow-diagram-line"] = { background = "muted-foreground/55" },
		[".flow-diagram-head"] = { color = "muted-foreground" },
		[".flow-diagram-link-label"] = { color = "muted-foreground", font_size = 10 },
		[".flow-diagram-group"] = { background = "accent/8", border = "all", border_color = "accent/30", radius = 12 },
		[".flow-diagram-group-label"] = { color = "accent", font_size = 10, weight = "medium", padding = { 4, 8 } },
		[".flow-diagram-token"] = { radius = 5, border = "all", border_color = "background" },
		[".flow-diagram-token-plain"] = { background = "muted-foreground" },
		[".flow-diagram-token-accent"] = { background = "accent" },
		[".flow-diagram-token-success"] = { background = "success" },
		[".flow-diagram-token-warning"] = { background = "warning" },
		[".flow-diagram-token-danger"] = { background = "danger" },
		[".flow-diagram-node"] = {
			background = "background",
			border = "all",
			border_color = "muted-foreground/60",
			radius = 12,
			padding = { x = 9 },
			shadow = "small",
		},
		[".flow-diagram-node:hover"] = { border_color = "accent" },
		[".flow-diagram-node:selected"] = { border_color = "accent", background = "accent/8", shadow = "none" },
		[".flow-diagram-node-accent"] = { border_color = "accent" },
		[".flow-diagram-node-success"] = { border_color = "success" },
		[".flow-diagram-node-warning"] = { border_color = "warning" },
		[".flow-diagram-node-danger"] = { border_color = "danger" },
		[".flow-diagram-node-planned"] = { opacity = 0.55, shadow = "none" },
		[".flow-diagram-top"] = { gap = 8 },
		[".flow-diagram-icon"] = { radius = 7, background = "muted", color = "muted-foreground" },
		[".flow-diagram-icon-accent"] = { background = "accent/15", color = "accent" },
		[".flow-diagram-icon-success"] = { background = "success/15", color = "success" },
		[".flow-diagram-icon-warning"] = { background = "warning/15", color = "warning" },
		[".flow-diagram-icon-danger"] = { background = "danger/15", color = "danger" },
		[".flow-diagram-label"] = { weight = "medium" },
		[".flow-diagram-sublabel"] = { color = "muted-foreground", font_size = 11 },
		[".flow-diagram-meter"] = { gap = 6, padding = { top = 4 } },
		[".flow-diagram-track"] = { background = "muted", radius = 2, clip = true },
		[".flow-diagram-fill-plain"] = { background = "muted-foreground", radius = 2 },
		[".flow-diagram-fill-accent"] = { background = "accent", radius = 2 },
		[".flow-diagram-fill-success"] = { background = "success", radius = 2 },
		[".flow-diagram-fill-warning"] = { background = "warning", radius = 2 },
		[".flow-diagram-fill-danger"] = { background = "danger", radius = 2 },
		[".flow-diagram-meter-text"] = { font_size = 11, color = "muted-foreground" },
		[".flow-diagram-text-plain"] = { color = "muted-foreground" },
		[".flow-diagram-text-accent"] = { color = "accent" },
		[".flow-diagram-text-success"] = { color = "success" },
		[".flow-diagram-text-warning"] = { color = "warning" },
		[".flow-diagram-text-danger"] = { color = "danger" },
		[".flow-diagram-note"] = { radius = 6, padding = { 1, 7 }, font_size = 11, background = "background", color = "muted-foreground" },
	},
	example = {
		nodes = {
			{ id = "web", label = "Web app", icon = "globe", col = 0, row = 0 },
			{ id = "api", label = "API", sublabel = "request handler", icon = "server", col = 1, row = 0, tone = "accent" },
			{ id = "db", label = "Database", icon = "database", col = 2, row = 0 },
			{ id = "cache", label = "Cache", icon = "clock", col = 1, row = 1, meter = { text = "hit 4 in 5", level = 0.8, tone = "success" } },
		},
		links = EXAMPLE_LINKS,
		groups = { { label = "Backend", nodes = { "api", "db", "cache" } } },
		flows = { { path = { "web", "api", "cache" }, tokens = { "warning", "warning", "success" } } },
		time = 1.2,
		selected = "api",
		event = "node",
	},
	usage = [[local flow_diagram = use "flow-diagram"

flow_diagram {
	nodes = {
		{ id = "web", label = "Web app", icon = "globe", col = 0, row = 0 },
		{ id = "api", label = "API", icon = "server", col = 1, row = 0 },
		{ id = "db", label = "Database", icon = "database", col = 2, row = 0 },
	},
	links = { { from = "web", to = "api", label = "HTTPS" }, { from = "api", to = "db" } },
	flows = { { path = { "web", "api", "db" }, tokens = { "warning", "warning", "danger" } } },
	time = state.seconds,      -- from a clock the mod owns
	selected = state.node,
	event = "node",            -- a click sends { node = id }
}]],

	build = function(props: Props)
		local size = SIZES[props.size or "normal"] or SIZES.normal
		local cols, rows = props.columns or 1, props.rows or 1
		for _, node in props.nodes do
			cols, rows = math.max(cols, node.col + 1), math.max(rows, node.row + 1)
		end
		local at: { [string]: Point }, kids: { any } = {}, {}
		for _, node in props.nodes do
			at[node.id] = { x = (node.col + 0.5) / cols, y = (node.row + 0.5) / rows, col = node.col, row = node.row }
		end
		for _, group in ipairs(props.groups or {}) do
			local left, top, right, bottom = cols, rows, -1, -1
			for _, id in group.nodes do
				local where = at[id]
				if where then
					left, top = math.min(left, where.col), math.min(top, where.row)
					right, bottom = math.max(right, where.col), math.max(bottom, where.row)
				end
			end
			if right >= 0 then
				table.insert(
					kids,
					ui.column {
						class = "flow-diagram-group",
						area = {
							left = (left + 0.04) / cols,
							top = (top + 0.04) / rows,
							width = (right - left + 0.92) / cols,
							height = (bottom - top + 0.92) / rows,
						},
						ui.row { ui.text { group.label, class = "flow-diagram-group-label" } },
					}
				)
			end
		end
		for _, each in ipairs(props.links or {}) do
			local a, b = at[each.from], at[each.to]
			-- Only level and upright links can be drawn; a slanted one is left out.
			if a and b and (a.row == b.row or a.col == b.col) then
				table.insert(kids, link(a, b, size, each.label))
			end
		end
		if props.time then
			for _, flow in ipairs(props.flows or {}) do
				tokens(flow, at, props.time, props.speed or 0.55, kids)
			end
		end
		for _, node in props.nodes do
			local where = at[node.id]
			table.insert(
				kids,
				ui.column {
					key = node.id,
					area = { left = where.x - 0.5 / cols, top = where.y - 0.5 / rows, width = 1 / cols, height = 1 / rows },
					align = "center",
					justify = "center",
					gap = "none",
					-- An empty slot above balances the note below: the card stays on the links.
					ui.row { height = size.slot },
					card(node, size, props),
					ui.row {
						height = size.slot,
						align = "end",
						justify = "center",
						node.note and ui.row { class = "flow-diagram-note", hug = true, ui.text { node.note } },
					},
				}
			)
		end
		return ui.row { class = "flow-diagram", height = props.height or rows * 120, table.unpack(kids) }
	end,
}
