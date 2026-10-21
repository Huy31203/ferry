/** A result card as sent by Rust. Sizes are bytes; `thumb` is an image URL or null. */
export interface CardData {
  id: string;
  name: string;
  ext: string;
  before: number;
  after: number;
  detail: string;
  thumb: string | null;
}

export const isMac = navigator.userAgent.includes('Mac');

/** Tag <html> with os-mac / os-windows so theme.css can switch fonts and radii. */
export function applyOsClass(): void {
  document.documentElement.classList.add(isMac ? 'os-mac' : 'os-windows');
}
