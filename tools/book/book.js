// The docs site's additions to mdBook's pages (book.toml): links to the
// site's landing page, to the gallery, which runs in the browser, and to
// the API documentation, beside the repository's link.
(() => {
    const buttons = document.querySelector(".right-buttons");
    if (!buttons) return;
    const links = [
        ["https://quazardous.github.io/egui-pop/", "Home", "What egui-pop is, with screenshots"],
        ["https://quazardous.github.io/egui-pop/gallery/", "Gallery", "The gallery, live in the browser"],
        ["https://quazardous.github.io/egui-pop/api/", "API", "The API documentation, every feature on"],
    ];
    for (const [href, text, title] of links.reverse()) {
        const a = document.createElement("a");
        a.href = href;
        a.className = "gallery-link";
        a.title = title;
        a.textContent = text;
        buttons.prepend(a);
    }
})();
