(() => {
  "use strict";

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  // Navigation: a quieter surface once the page moves.
  const nav = document.getElementById("nav");
  let navTicking = false;
  const updateNav = () => {
    if (nav) nav.classList.toggle("is-scrolled", window.scrollY > 12);
    navTicking = false;
  };
  if (nav) {
    updateNav();
    window.addEventListener(
      "scroll",
      () => {
        if (!navTicking) {
          window.requestAnimationFrame(updateNav);
          navTicking = true;
        }
      },
      { passive: true }
    );
  }

  // Section reveals. Content is visible when scripting is off; JS opts in.
  const reveals = [...document.querySelectorAll(".reveal")];
  const showAllReveals = () => {
    reveals.forEach((el) => el.classList.add("is-in"));
  };
  if (!("IntersectionObserver" in window) || reduceMotion.matches) {
    showAllReveals();
  } else {
    const revealObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          entry.target.classList.add("is-in");
          revealObserver.unobserve(entry.target);
        });
      },
      { threshold: 0.12, rootMargin: "0px 0px -8% 0px" }
    );
    reveals.forEach((el) => revealObserver.observe(el));
  }

  // The statement types itself in, one character at a time.
  const statement = document.getElementById("statement-title");
  const chars = statement ? [...statement.querySelectorAll(".ch")] : [];
  const showStatement = () => {
    chars.forEach((ch) => {
      ch.style.transitionDelay = "0ms";
      ch.classList.add("on");
    });
  };
  if (chars.length) {
    if (!("IntersectionObserver" in window) || reduceMotion.matches) {
      showStatement();
    } else {
      const statementObserver = new IntersectionObserver(
        (entries) => {
          if (!entries.some((entry) => entry.isIntersecting)) return;
          chars.forEach((ch, index) => {
            ch.style.transitionDelay = `${index * 26}ms`;
            ch.classList.add("on");
          });
          statementObserver.disconnect();
        },
        { threshold: 0.3 }
      );
      statementObserver.observe(statement);
    }
  }

  // Theme switcher. Real screenshots, one frame, a quiet crossfade.
  const themes = {
    paper: {
      src: "screenshots/cards/paper.webp",
      alt: "默认主题，纸与墨，距离下班还有两小时三十七分",
    },
    sea: {
      src: "screenshots/cards/sea.webp",
      alt: "大海主题，日落与海，距离下班还有两小时三十七分",
    },
    aurora: {
      src: "screenshots/cards/aurora.webp",
      alt: "极光主题，夜与光，距离下班还有两小时三十七分",
    },
    ridge: {
      src: "screenshots/cards/ridge.webp",
      alt: "山影主题，云与山，距离下班还有两小时三十七分",
    },
  };
  const tabs = [...document.querySelectorAll(".theme-tab")];
  const frame = document.getElementById("theme-frame");
  const image = document.getElementById("theme-image");

  if (tabs.length && frame && image) {
    Object.values(themes).forEach((theme) => {
      const preload = new Image();
      preload.src = theme.src;
    });

    let switchId = 0;
    const selectTheme = (key, focusTab = false) => {
      const theme = themes[key];
      const tab = tabs.find((item) => item.dataset.theme === key);
      if (!theme || !tab) return;

      tabs.forEach((item) => {
        const active = item === tab;
        item.setAttribute("aria-selected", String(active));
        item.tabIndex = active ? 0 : -1;
      });
      if (focusTab) tab.focus();
      frame.setAttribute("aria-labelledby", tab.id);

      const id = ++switchId;
      frame.classList.add("is-switching");
      window.setTimeout(() => {
        if (id !== switchId) return;
        image.src = theme.src;
        image.alt = theme.alt;
        const settle = () => {
          if (id === switchId) frame.classList.remove("is-switching");
        };
        if (image.complete) {
          window.requestAnimationFrame(settle);
        } else {
          image.addEventListener("load", settle, { once: true });
          image.addEventListener("error", settle, { once: true });
        }
      }, 170);
    };

    tabs.forEach((tab) => {
      tab.addEventListener("click", () => selectTheme(tab.dataset.theme));
      tab.addEventListener("keydown", (event) => {
        const index = tabs.indexOf(tab);
        let next = -1;
        if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % tabs.length;
        if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index - 1 + tabs.length) % tabs.length;
        if (event.key === "Home") next = 0;
        if (event.key === "End") next = tabs.length - 1;
        if (next >= 0) {
          event.preventDefault();
          selectTheme(tabs[next].dataset.theme, true);
        }
      });
    });
  }

  // If the motion preference changes, make sure nothing stays hidden.
  const onMotionChange = () => {
    if (!reduceMotion.matches) return;
    showAllReveals();
    showStatement();
  };
  if (typeof reduceMotion.addEventListener === "function") {
    reduceMotion.addEventListener("change", onMotionChange);
  }
})();
