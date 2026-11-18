var __megi_worker_listeners = [];
function addEventListener(type, callback) { if (type === "message" && typeof callback === "function" && __megi_worker_listeners.length < 64) { __megi_worker_listeners.push(callback); } }
function postMessage(d) { __megi_worker_data = d; }
postMessage("boot");
