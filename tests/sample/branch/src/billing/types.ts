export interface Fee {
  id: string;
  amountMills: number;
}

export interface Insertion {
  fee: Fee;
  reason: string;
}
