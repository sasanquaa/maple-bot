// all the getAttribute/setAttribute are for that one dioxus bug
// spagetti for now, i don't care :(

// const MAGNIFIER_OFFSET_X = "magnifierOffsetX";
// const MAGNIFIER_OFFSET_Y = "magnifierOffsetY";
// const MAGNIFYING = "magnifying";
// const MAGNIFIER_SIZE = 160;
// const MAGNIFIER_SCALE = 35;

// let magnifier = document.getElementById("canvas-minimap-magnifier");
// let magnifierCtx = magnifier.getContext("2d");
// let magnifierOffsetX = parseInt(
//   magnifier.getAttribute(MAGNIFIER_OFFSET_X) || "0",
// );
// let magnifierOffsetY = parseInt(
//   magnifier.getAttribute(MAGNIFIER_OFFSET_Y) || "0",
// );
// let magnifying = magnifier.getAttribute(MAGNIFYING) === "true";

let minimap = document.getElementById("canvas-minimap");
let minimapCtx = minimap.getContext("2d");
let lastWidth = minimap.width;
let lastHeight = minimap.height;

// let minimapMagnify = (e) => {
//   let bbox = minimap.getBoundingClientRect();
//   let offsetX = bbox.left;
//   let offsetY = bbox.top;
//   let x = e.clientX - offsetX;
//   let y = e.clientY - offsetY;
//   let xCanvas = Math.floor((x / bbox.width) * lastWidth);
//   let yCanvas = Math.floor((y / bbox.height) * lastHeight);

//   magnifier.style.left = x - MAGNIFIER_SIZE / 2 + "px";
//   magnifier.style.top = y - MAGNIFIER_SIZE / 2 + "px";
//   magnifierOffsetX = xCanvas - MAGNIFIER_SCALE / 2;
//   magnifierOffsetY = yCanvas - MAGNIFIER_SCALE / 2;
//   magnifier.setAttribute(MAGNIFIER_OFFSET_X, magnifierOffsetX.toString());
//   magnifier.setAttribute(MAGNIFIER_OFFSET_Y, magnifierOffsetY.toString());
// };
// window.addEventListener("mouseup", (_) => {
//   magnifying = false;
//   magnifier.setAttribute(MAGNIFYING, "false");
//   magnifier.classList.add("hidden");
// });
// minimap.addEventListener("mousedown", (e) => {
//   e.preventDefault();
//   e.stopPropagation();
//   magnifying = true;
//   magnifier.setAttribute(MAGNIFYING, "true");
//   magnifier.classList.remove("hidden");
//   minimapMagnify(e);
// });
// minimap.addEventListener("mousemove", (e) => {
//   if (magnifying) {
//     minimapMagnify(e);
//   }
// });

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
  // else if (magnifying) {
  //   magnifierCtx.clearRect(0, 0, MAGNIFIER_SIZE, MAGNIFIER_SIZE);
  //   magnifierCtx.drawImage(
  //     bitmap,
  //     magnifierOffsetX,
  //     magnifierOffsetY,
  //     MAGNIFIER_SCALE,
  //     MAGNIFIER_SCALE,
  //     0,
  //     0,
  //     MAGNIFIER_SIZE,
  //     MAGNIFIER_SIZE,
  //   );
  // }
}
