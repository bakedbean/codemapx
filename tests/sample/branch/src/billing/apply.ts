import type { Insertion } from '@/billing/types';
import { millsToDecimal } from './mills';

export function applyChanges(insertions: Insertion[]): string[] {
  return insertions.map((ins) => `${ins.fee.id}=${millsToDecimal(ins.fee.amountMills)}`);
}
