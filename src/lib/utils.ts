import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function settingMap(settings: { key: string; value: string }[]) {
  return Object.fromEntries(settings.map((s) => [s.key, s.value]));
}
