import { b } from "./cycle_b.js";

export const a = 1;
export function sum() { return a + b; }
