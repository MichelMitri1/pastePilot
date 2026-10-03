/** Turns an image file/blob into a downscaled data URL (keeps uploads small and fast). */
export async function toDataUrl(blob: Blob, maxSide = 1600): Promise<string> {
  const url = URL.createObjectURL(blob);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const i = new Image();
      i.onload = () => resolve(i);
      i.onerror = () => reject(new Error("That file isn't an image."));
      i.src = url;
    });
    const scale = Math.min(1, maxSide / Math.max(img.naturalWidth, img.naturalHeight));
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(img.naturalWidth * scale);
    canvas.height = Math.round(img.naturalHeight * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Couldn't read the image.");
    ctx.fillStyle = "#fff"; // transparent PNGs → white, not black, as JPEG
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    return canvas.toDataURL("image/jpeg", 0.88);
  } finally {
    URL.revokeObjectURL(url);
  }
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
