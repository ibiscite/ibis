/** Calendar facts only. Rust owns every date-based eligibility decision. */
export function localMoment() {
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth() + 1, day: now.getDate() };
}
