document.getElementById("header").addEventListener("mousedown", (e) => {
  if (e.button == 0) {
    dioxus.send(true);
    dioxus.send(false);
  }
});
