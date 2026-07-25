const _shortDay = new Intl.DateTimeFormat(navigator.language, {
  month: "short",
  day: "numeric",
});

/** Format a "YYYY-MM-DD" string as a short locale date, e.g. "Jul 24". */
export function formatShortDay(day: string): string {
  const parts = day.split("-").map(Number);
  if (parts.length < 3 || parts.some((n) => Number.isNaN(n))) return day;
  const [y, m, d] = parts;
  return _shortDay.format(new Date(y, m - 1, d));
}
