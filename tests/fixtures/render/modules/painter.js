import { SIZE } from "./geometry.js";

export function paint(id, color) {
  const element = document.getElementById(id);
  element.style.width = SIZE + "px";
  element.style.height = "20px";
  element.style.backgroundColor = color;
}
