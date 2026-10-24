/** A card's box for the share picker: the stack page's CSS px, and the side it opens toward. */
export interface ShareAnchor {
  x: number;
  y: number;
  width: number;
  height: number;
  toward: 'left' | 'right';
}

/** The picker opens toward the screen centre: right of a card in a left corner, else left. */
export function shareAnchor(rect: Pick<DOMRect, 'x' | 'y' | 'width' | 'height'>, railRight: boolean): ShareAnchor {
  return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, toward: railRight ? 'right' : 'left' };
}
