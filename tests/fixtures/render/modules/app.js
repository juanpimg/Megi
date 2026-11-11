import { SIZE, area } from "./geometry.js";
import * as colors from "./colors.js";
import meta from "./meta.js";

const out = document.getElementById("out");
const value = area(4, 10);
out.style.width = SIZE + "px";
out.style.height = "20px";
out.style.backgroundColor = colors.GREEN;
out.setAttribute("data-area", String(value));
out.setAttribute("data-meta", meta.tag);
