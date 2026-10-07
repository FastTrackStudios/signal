// A set is an event on a date, with a title only when the night has one:
// "HSM · Tue 6 Oct", or "Worship Night" (HSM · Tue 6 Oct). Its stored name is
// built from those, the way sets have always been named — "HSM 10-6-26
// Worship Night" — so files and the rig stay as they are; existing names
// are read back into their parts.

export interface SetMeta {
  /** The recurring event: HSM, XR Wednesday, Kids Ministry… */
  event: string;
  /** yyyy-mm-dd, or "" when the set has none. */
  date: string;
  /** Only for a night with a name of its own. */
  title: string;
}

/** "HSM 10-6-26 Worship Night" → HSM · 2026-10-06 · Worship Night. A name
 *  without a date is all event. */
export function parseSetName(name: string): SetMeta {
  const m = name.trim().match(/^(.*?)\s+(\d{1,2})-(\d{1,2})-(\d{2}|\d{4})(?:\s+(.*))?$/);
  if (!m) return { event: name.trim(), date: "", title: "" };
  const [, event, mo, d, y, title] = m;
  const year = y.length === 2 ? 2000 + Number(y) : Number(y);
  const date = `${year}-${String(mo).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
  return { event: event.trim(), date, title: (title ?? "").trim() };
}

/** The stored name, in the house style: event, M-D-YY, title. */
export function setName(m: SetMeta): string {
  const parts = [m.event.trim()];
  if (m.date) {
    const [y, mo, d] = m.date.split("-").map(Number);
    parts.push(`${mo}-${d}-${String(y).slice(-2)}`);
  }
  if (m.title.trim()) parts.push(m.title.trim());
  return parts.filter(Boolean).join(" ");
}

/** What the set is called on screen: its title, else its event. */
export function setHeading(m: SetMeta): string {
  return m.title.trim() || m.event.trim() || "Untitled set";
}

export function toDate(iso: string): Date | null {
  if (!iso) return null;
  const [y, mo, d] = iso.split("-").map(Number);
  return new Date(y, mo - 1, d);
}

export function isoOf(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function addDays(iso: string, days: number): string {
  const d = toDate(iso) ?? new Date();
  d.setDate(d.getDate() + days);
  return isoOf(d);
}

export const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
export const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** "Tue 6 Oct" (with the year when it isn't this one). */
export function dateLabel(iso: string, today = new Date()): string {
  const d = toDate(iso);
  if (!d) return "No date";
  const year = d.getFullYear() !== today.getFullYear() ? ` ${d.getFullYear()}` : "";
  return `${WEEKDAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]}${year}`;
}

/** "Tonight", "Tomorrow", "Yesterday", "In 5 days", "3 weeks ago". */
export function whenLabel(iso: string, today = new Date()): string {
  const d = toDate(iso);
  if (!d) return "";
  const t = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const days = Math.round((d.getTime() - t.getTime()) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days === -1) return "Yesterday";
  if (days > 1 && days < 7) return `In ${days} days`;
  if (days >= 7 && days < 14) return "Next week";
  if (days >= 14) return `In ${Math.round(days / 7)} weeks`;
  if (days > -7) return `${-days} days ago`;
  if (days > -14) return "Last week";
  return `${Math.round(-days / 7)} weeks ago`;
}

/** The next date an event usually falls on: a week after its latest set, or
 *  today when it has none (or the latest is long past). */
export function nextDateFor(event: string, sets: SetMeta[], today = new Date()): string {
  const dates = sets.filter((s) => s.event.toLowerCase() === event.toLowerCase() && s.date).map((s) => s.date).sort();
  const todayIso = isoOf(today);
  if (!dates.length) return todayIso;
  let next = addDays(dates[dates.length - 1], 7);
  while (next < todayIso) next = addDays(next, 7);
  return next;
}
