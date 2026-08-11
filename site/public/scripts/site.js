const toggle = document.querySelector(".nav-toggle");
const nav = document.querySelector("#site-nav");

if (toggle instanceof HTMLButtonElement && nav instanceof HTMLElement) {
  toggle.addEventListener("click", () => {
    const open = toggle.getAttribute("aria-expanded") === "true";
    toggle.setAttribute("aria-expanded", String(!open));
    nav.classList.toggle("is-open", !open);
  });
}

const demo = document.querySelector("[data-demo]");
if (demo instanceof HTMLElement) {
  const buttons = [...demo.querySelectorAll("[data-demo-step]")];
  const panels = [...demo.querySelectorAll("[data-demo-panel]")];
  const select = (step) => {
    for (const button of buttons) {
      const selected = button.getAttribute("data-demo-step") === step;
      button.setAttribute("aria-selected", String(selected));
      button.classList.toggle("active", selected);
    }
    for (const panel of panels) {
      const selected = panel.getAttribute("data-demo-panel") === step;
      panel.hidden = !selected;
    }
  };
  for (const button of buttons) {
    button.addEventListener("click", () => select(button.getAttribute("data-demo-step")));
  }
  select("summary");
}
