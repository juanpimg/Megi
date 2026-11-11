const target = document.getElementById("target");
const second = document.getElementById("second");
const observer = new MutationObserver(() => {
  target.setAttribute("data-observed", "yes");
});
observer.observe(target, { attributes: true });
target.setAttribute("data-ready", "1");
setTimeout(() => {
  second.style.backgroundColor = "#00ff00";
}, 10);
