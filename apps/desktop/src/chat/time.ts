export function duration(seconds: number): string {
  if (seconds > 0 && seconds < 1) return '<1s';
  const value = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  const remainder = value % 60;
  return hours ? `${hours}h ${minutes}m` : minutes ? `${minutes}m ${remainder}s` : `${remainder}s`;
}
export function clockTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}
export function fullTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'long' });
}
