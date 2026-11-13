// Adds an event handler and marks the element, so a test (and the browser check) can tell a control
// that works from one that only looks like it does.
export function listen<K extends keyof HTMLElementEventMap>(node: HTMLElement, type: K, handler: (event: HTMLElementEventMap[K]) => void): void {
  node.addEventListener(type, handler);
  const seen = node.getAttribute("data-handler");
  node.setAttribute("data-handler", seen === null ? type : `${seen} ${type}`);
}
