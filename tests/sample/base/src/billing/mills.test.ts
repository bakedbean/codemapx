import { toMills } from './mills';

test('toMills', () => {
  expect(toMills(1.5)).toBe(1500);
});
