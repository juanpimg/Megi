import { a } from "./cycle_a.js";

export const b = 2;
export function check() { return a + b; }
