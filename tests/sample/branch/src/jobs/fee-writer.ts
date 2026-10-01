import type { Fee } from '@/billing/types';

export function writeFee(fee: Fee): void {
  console.log('write', fee.id, fee.amountMills);
}
