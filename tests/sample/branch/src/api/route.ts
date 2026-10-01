import type { Fee } from '@/billing/types';
import { applyChanges } from '../billing/apply';

export function handle(fees: Fee[]): string[] {
  return applyChanges(fees.map((fee) => ({ fee, reason: 'regenerate' })));
}
