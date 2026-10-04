/** Reads a file as a data: URL. (blob: URLs are blocked by the app's security policy; data: is allowed.) */
const readAsDataUrl = (blob: Blob) =>
  new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("Couldn't read that image."));
    reader.readAsDataURL(blob);
  });

/** Turns an image file/blob into a downscaled JPEG data URL (keeps uploads small and fast). */
export async function toDataUrl(blob: Blob, maxSide = 1600): Promise<string> {
  const source = await readAsDataUrl(blob);
  const img = await new Promise<HTMLImageElement>((resolve, reject) => {
    const i = new Image();
    i.onload = () => resolve(i);
    i.onerror = () => reject(new Error("Couldn't open that image. Try a PNG or JPEG screenshot."));
    i.src = source;
  });
  const scale = Math.min(1, maxSide / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) return source; // can't downscale: send the original
  ctx.fillStyle = "#fff"; // transparent PNGs → white, not black, as JPEG
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/jpeg", 0.88);
}

export const imagesFrom = (items: DataTransferItemList | FileList | null): File[] => {
  if (!items) return [];
  const out: File[] = [];
  for (const it of Array.from(items as ArrayLike<DataTransferItem | File>)) {
    const file = it instanceof File ? it : it.kind === "file" ? it.getAsFile() : null;
    if (file && file.type.startsWith("image/")) out.push(file);
  }
  return out;
};
