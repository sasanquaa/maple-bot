let minimap = document.getElementById("canvas-minimap");
let minimapCtx = minimap.getContext("2d");
let lastWidth = minimap.width;
let lastHeight = minimap.height;

while (true) {
  let [buffer, width, height] = await dioxus.recv();
  let data = new ImageData(new Uint8ClampedArray(buffer), width, height);
  let bitmap = await createImageBitmap(data);
  minimapCtx.drawImage(bitmap, 0, 0);
  if (lastWidth != width || lastHeight != height) {
    lastWidth = width;
    lastHeight = height;
    minimap.width = width;
    minimap.height = height;
  }
}
