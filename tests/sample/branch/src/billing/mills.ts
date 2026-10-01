export function toMills(dollars: number): number {
  return Math.round(dollars * 1000);
}

export function millsToDecimal(mills: number): string {
  if (!Number.isInteger(mills)) throw new Error(`not integer mills: ${mills}`);
  return (mills / 1000).toFixed(3);
}
