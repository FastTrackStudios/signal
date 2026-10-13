-- Tempo Oracle Exporter for REAPER
-- Exports dense tempo-map ground truth for dawfile-reaper parity testing.
--
-- Output files (next to current project .RPP):
--   tempo_oracle_full.tsv
--   tempo_curve_samples.tsv
--   tempo_qn_to_time.tsv
--   tempo_pt_raw.tsv
--
-- Run inside REAPER with the target project open.

local proj = 0
local sep = "\t"

local function msg(s)
  reaper.ShowConsoleMsg(tostring(s) .. "\n")
end

local function q(v)
  if v == nil then return "" end
  if type(v) == "boolean" then return v and "true" or "false" end
  return tostring(v)
end

local function join_row(t)
  for i = 1, #t do t[i] = q(t[i]) end
  return table.concat(t, sep)
end

local function write_lines(path, lines)
  local f, err = io.open(path, "w")
  if not f then error("Failed to open " .. path .. ": " .. tostring(err)) end
  for i = 1, #lines do
    f:write(lines[i], "\n")
  end
  f:close()
end

local function tokenize(line)
  local out = {}
  local i = 1
  local n = #line
  while i <= n do
    while i <= n and line:sub(i, i):match("%s") do i = i + 1 end
    if i > n then break end

    local ch = line:sub(i, i)
    if ch == '"' then
      i = i + 1
      local j = i
      while j <= n and line:sub(j, j) ~= '"' do j = j + 1 end
      table.insert(out, line:sub(i, j - 1))
      i = j + 1
    else
      local j = i
      while j <= n and not line:sub(j, j):match("%s") do j = j + 1 end
      table.insert(out, line:sub(i, j - 1))
      i = j
    end
  end
  return out
end

local function parse_pt_lines(rpp_path)
  local f = io.open(rpp_path, "r")
  if not f then return {} end

  local pts = {}
  local in_tempo = false
  local line_no = 0
  for line in f:lines() do
    line_no = line_no + 1
    if line:find("<TEMPOENVEX", 1, true) then
      in_tempo = true
    elseif in_tempo and line:match("^%s*>") then
      in_tempo = false
    elseif in_tempo then
      local trimmed = line:match("^%s*(.-)%s*$") or line
      if trimmed:sub(1, 2) == "PT" then
        local toks = tokenize(trimmed)
        local pt = {
          line_no = line_no,
          raw = trimmed,
          tok_count = #toks,
          t = tonumber(toks[2]),
          bpm = tonumber(toks[3]),
          shape = tonumber(toks[4]),
          ts_encoded = tonumber(toks[5]),
          selected = tonumber(toks[6]),
          unk1 = tonumber(toks[7]),
          bezier = tonumber(toks[8]),
        }
        table.insert(pts, pt)
      end
    end
  end
  f:close()

  table.sort(pts, function(a, b)
    return (a.t or 0) < (b.t or 0)
  end)
  return pts
end

local function find_pt_for_time(pts, t)
  local best_idx, best_d = nil, 1e9
  for i = 1, #pts do
    local d = math.abs((pts[i].t or 0) - t)
    if d < best_d then
      best_d = d
      best_idx = i
    end
  end
  if best_d <= 1e-6 then return best_idx end
  return nil
end

local function inst_bpm_fd(time, dt)
  local e = math.max(1e-6, math.min(1e-3, dt * 1e-4))
  local t0 = math.max(0, time - e)
  local t1 = time + e
  local q0 = reaper.TimeMap2_timeToQN(proj, t0)
  local q1 = reaper.TimeMap2_timeToQN(proj, t1)
  return (q1 - q0) / (t1 - t0) * 60.0
end

local function tempo_info(idx)
  local ok, timepos, measurepos, beatpos, bpm, ts_num, ts_den, linear =
    reaper.GetTempoTimeSigMarker(proj, idx)
  if not ok then return nil end
  return {
    idx = idx,
    time = timepos,
    measurepos = measurepos,
    beatpos = beatpos,
    bpm = bpm,
    ts_num = ts_num,
    ts_den = ts_den,
    linear = linear,
    qn = reaper.TimeMap2_timeToQN(proj, timepos),
    ruler_mbs = reaper.format_timestr_pos(timepos, "", 2),
    ruler_time = reaper.format_timestr_pos(timepos, "", 0),
  }
end

local retval, proj_path = reaper.EnumProjects(-1, "")
if not retval or not proj_path or proj_path == "" then
  error("No active project path. Save the project first.")
end

local out_dir = proj_path:match("^(.*)[/\\]") or "."
local pt_lines = parse_pt_lines(proj_path)

local pt_raw = {
  join_row({"PT_IDX", "LINE_NO", "TIME", "BPM", "SHAPE", "TS_ENCODED", "SELECTED", "UNK1", "BEZIER", "TOK_COUNT", "RAW"})
}
for i = 1, #pt_lines do
  local p = pt_lines[i]
  table.insert(pt_raw, join_row({
    i - 1, p.line_no, p.t, p.bpm, p.shape, p.ts_encoded, p.selected, p.unk1, p.bezier, p.tok_count, p.raw
  }))
end

local tempo_count = reaper.CountTempoTimeSigMarkers(proj)
local tempos = {}
for i = 0, tempo_count - 1 do
  local ti = tempo_info(i)
  if ti then
    ti.pt_idx = find_pt_for_time(pt_lines, ti.time)
    local pt = ti.pt_idx and pt_lines[ti.pt_idx] or nil
    ti.shape = pt and pt.shape or nil
    ti.bezier = pt and pt.bezier or nil
    ti.ts_encoded = pt and pt.ts_encoded or nil
    table.insert(tempos, ti)
  end
end

local marker_lines = {
  join_row({
    "TYPE", "IDX", "NAME", "TIME", "QN", "RULER_MBS", "RULER_TIME",
    "MEASUREPOS", "BEATPOS", "BPM", "LINEAR", "TS_NUM", "TS_DEN",
    "PT_IDX", "PT_SHAPE", "PT_BEZIER", "PT_TS_ENCODED"
  })
}

for i = 1, #tempos do
  local t = tempos[i]
  table.insert(marker_lines, join_row({
    "TEMPO", t.idx, "", t.time, t.qn, t.ruler_mbs, t.ruler_time,
    t.measurepos, t.beatpos, t.bpm, t.linear, t.ts_num, t.ts_den,
    t.pt_idx and (t.pt_idx - 1) or "", t.shape, t.bezier, t.ts_encoded
  }))
end

local _, num_markers, num_regions = reaper.CountProjectMarkers(proj)
local total = num_markers + num_regions
for i = 0, total - 1 do
  local ok, isrgn, pos, rgnend, name, idx = reaper.EnumProjectMarkers3(proj, i)
  if ok then
    local qn = reaper.TimeMap2_timeToQN(proj, pos)
    local ruler_mbs = reaper.format_timestr_pos(pos, "", 2)
    local ruler_time = reaper.format_timestr_pos(pos, "", 0)
    table.insert(marker_lines, join_row({
      isrgn and "REGION" or "MARKER", idx, name or "", pos, qn, ruler_mbs, ruler_time,
      "", "", "", "", "", "", "", "", "", ""
    }))
  end
end

local curve_lines = {
  join_row({
    "SEG_IDX", "T0", "T1", "DT", "QN0", "QN1", "DQN",
    "BPM0", "BPM1", "LINEAR0", "LINEAR1", "TS_NUM0", "TS_DEN0",
    "PT_IDX0", "PT_SHAPE0", "PT_BEZIER0", "PT_TS_ENCODED0",
    "PT_IDX1", "PT_SHAPE1", "PT_BEZIER1", "PT_TS_ENCODED1",
    "U", "TIME", "QN", "QN_MINUS_QN0", "FD_BPM", "RULER_MBS"
  })
}

local u_step = 0.0025 -- 401 samples per segment
for i = 1, #tempos - 1 do
  local a, b = tempos[i], tempos[i + 1]
  local dt = b.time - a.time
  local q0, q1 = a.qn, b.qn
  local dqn = q1 - q0
  local n = math.floor(1.0 / u_step + 0.5)
  for s = 0, n do
    local u = s / n
    local time = a.time + dt * u
    local qn = reaper.TimeMap2_timeToQN(proj, time)
    local fd_bpm = inst_bpm_fd(time, dt)
    local ruler_mbs = reaper.format_timestr_pos(time, "", 2)
    table.insert(curve_lines, join_row({
      i - 1,
      a.time, b.time, dt, q0, q1, dqn,
      a.bpm, b.bpm, a.linear, b.linear, a.ts_num, a.ts_den,
      a.pt_idx and (a.pt_idx - 1) or "", a.shape, a.bezier, a.ts_encoded,
      b.pt_idx and (b.pt_idx - 1) or "", b.shape, b.bezier, b.ts_encoded,
      string.format("%.6f", u),
      string.format("%.15f", time),
      string.format("%.15f", qn),
      string.format("%.15f", qn - q0),
      string.format("%.12f", fd_bpm),
      ruler_mbs
    }))
  end
end

local qn_lines = {
  join_row({"QN", "TIME", "RULER_MBS", "FD_BPM"})
}
local max_qn = 0.0
for i = 1, #tempos do
  if tempos[i].qn > max_qn then max_qn = tempos[i].qn end
end
max_qn = math.ceil(max_qn + 64)
for qn = 0, max_qn, 0.25 do
  local time = reaper.TimeMap2_QNToTime(proj, qn)
  local bpm = inst_bpm_fd(time, math.max(0.01, 1.0 / 480.0))
  local ruler_mbs = reaper.format_timestr_pos(time, "", 2)
  table.insert(qn_lines, join_row({
    string.format("%.6f", qn),
    string.format("%.15f", time),
    ruler_mbs,
    string.format("%.12f", bpm)
  }))
end

local f_oracle = out_dir .. "/tempo_oracle_full.tsv"
local f_curve = out_dir .. "/tempo_curve_samples.tsv"
local f_qn = out_dir .. "/tempo_qn_to_time.tsv"
local f_pt = out_dir .. "/tempo_pt_raw.tsv"

write_lines(f_oracle, marker_lines)
write_lines(f_curve, curve_lines)
write_lines(f_qn, qn_lines)
write_lines(f_pt, pt_raw)

reaper.ClearConsole()
msg("=== TEMPO ORACLE EXPORT COMPLETE ===")
msg("Project: " .. proj_path)
msg("Tempo markers: " .. tostring(#tempos))
msg("PT lines parsed: " .. tostring(#pt_lines))
msg("Wrote: " .. f_oracle)
msg("Wrote: " .. f_curve)
msg("Wrote: " .. f_qn)
msg("Wrote: " .. f_pt)
