import { convertFileSrc } from '@tauri-apps/api/core';

function load(path: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.crossOrigin = 'anonymous';
    image.onload = () => resolve(image);
    image.onerror = () => resolve(null);
    image.src = convertFileSrc(path);
  });
}

/** A small fan of up to three thumbnails, built ahead of time so a drag starts at once. */
export async function fanIcon(paths: string[]): Promise<string> {
  const images = (await Promise.all(paths.map(load))).filter(
    (image): image is HTMLImageElement => image !== null,
  );
  const canvas = document.createElement('canvas');
  canvas.width = 132;
  canvas.height = 104;
  const ctx = canvas.getContext('2d');
  if (!ctx) return '';
  const tilts = images.length === 1 ? [0] : images.length === 2 ? [-6, 6] : [-9, 0, 9];
  images.forEach((image, i) => {
    const scale = Math.min(84 / image.naturalWidth, 60 / image.naturalHeight);
    const w = image.naturalWidth * scale;
    const h = image.naturalHeight * scale;
    ctx.save();
    ctx.translate(66, 52);
    ctx.rotate((tilts[i] * Math.PI) / 180);
    ctx.shadowColor = 'rgba(0, 0, 0, 0.35)';
    ctx.shadowBlur = 6;
    ctx.shadowOffsetY = 2;
    ctx.fillStyle = '#fff';
    ctx.fillRect(-w / 2 - 3, -h / 2 - 3, w + 6, h + 6);
    ctx.shadowColor = 'transparent';
    ctx.drawImage(image, -w / 2, -h / 2, w, h);
    ctx.restore();
  });
  try {
    return canvas.toDataURL('image/png');
  } catch {
    return '';
  }
}
