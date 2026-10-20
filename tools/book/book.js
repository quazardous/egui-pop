// The docs site's additions to mdBook's pages (book.toml): a link to the
// gallery, which runs in the browser, beside the repository's link.
(() => {
    const buttons = document.querySelector(".right-buttons");
    if (!buttons) return;
    const a = document.createElement("a");
    a.href = "https://quazardous.github.io/egui-pop/";
    a.className = "gallery-link";
    a.title = "The gallery, live in the browser";
    a.textContent = "Gallery";
    buttons.prepend(a);
})();
