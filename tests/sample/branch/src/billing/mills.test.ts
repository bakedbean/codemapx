import { toMills, millsToDecimal } from './mills';

test('toMills', () => {
  expect(toMills(1.5)).toBe(1500);
});

test('millsToDecimal', () => {
  expect(millsToDecimal(1500)).toBe('1.500');
});
