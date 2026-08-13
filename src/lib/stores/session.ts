import { writable } from "svelte/store";
import type { SessionProjection } from "../types/contracts";

export const session = writable<SessionProjection | null>(null);
export const appError = writable<string | null>(null);

