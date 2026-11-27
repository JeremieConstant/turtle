// The header shows its hairline once the page has scrolled under it.
(() => {
  const header = document.querySelector('.site-header');
  if (!header) return;
  const update = () => header.toggleAttribute('data-scrolled', scrollY > 4);
  addEventListener('scroll', update, { passive: true });
  update();
})();
