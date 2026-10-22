/** Settings search (spec §7): every whitespace-separated word of `query` must appear in one of `fields`, in any case. */
export function matches(query: string, ...fields: string[]): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const text = fields.join(' ').toLowerCase();
  return words.every((word) => text.includes(word));
}
