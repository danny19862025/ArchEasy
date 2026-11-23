(() => {
  // --------------------------------------------------------------------------
  // Theme Toggle (Dark default, matches ArchEasy's signature look)
  // --------------------------------------------------------------------------
  const root = document.documentElement;
  const themeBtn = document.getElementById("theme-toggle");
  const sunIcon = document.getElementById("icon-sun");
  const moonIcon = document.getElementById("icon-moon");

  function applyTheme(theme) {
    root.setAttribute("data-theme", theme);
    if (sunIcon && moonIcon) {
      sunIcon.style.display = theme === "dark" ? "block" : "none";
      moonIcon.style.display = theme === "light" ? "block" : "none";
    }
    if (themeBtn) {
      themeBtn.setAttribute(
        "aria-label",
        theme === "dark" ? "Switch to light theme" : "Switch to dark theme"
      );
    }
  }

  const savedTheme = localStorage.getItem("archeasy-site-theme");
  const initialTheme =
    savedTheme === "light" || savedTheme === "dark" ? savedTheme : "dark";
  applyTheme(initialTheme);

  if (themeBtn) {
    themeBtn.addEventListener("click", () => {
      const next = root.getAttribute("data-theme") === "dark" ? "light" : "dark";
      localStorage.setItem("archeasy-site-theme", next);
      applyTheme(next);
    });
  }

  // --------------------------------------------------------------------------
  // Mobile Navigation
  // --------------------------------------------------------------------------
  const mobileBtn = document.getElementById("mobile-menu-btn");
  const navLinks = document.getElementById("nav-links");

  if (mobileBtn && navLinks) {
    mobileBtn.addEventListener("click", () => {
      const open = navLinks.classList.toggle("open");
      mobileBtn.setAttribute("aria-expanded", String(open));
    });

    navLinks.querySelectorAll("a").forEach((link) => {
      link.addEventListener("click", () => {
        navLinks.classList.remove("open");
        mobileBtn.setAttribute("aria-expanded", "false");
      });
    });
  }

  // --------------------------------------------------------------------------
  // Hero Screenshot Switcher + Lightbox
  // --------------------------------------------------------------------------
  const screenshots = {
    software: {
      src: "assets/software-centre.png",
      alt: "ArchEasy Software Centre with integrated workbench sidebar and multi-source search",
      caption:
        "Software Centre — unified Pacman, AUR, Flatpak, and GitHub search with the always-visible workbench sidebar."
    },
    configs: {
      src: "assets/configs-workbench.png",
      alt: "ArchEasy Configs page with built-in presets, dry-run preview, and backup controls",
      caption:
        "Configs & Presets — preview changes before applying, back up to ZIP or Google Drive, and browse Config Timeline diffs."
    },
    installed: {
      src: "assets/installed.png",
      alt: "ArchEasy Installed tab showing animated percentage circles for Pacman, AUR, and Flatpak",
      caption:
        "Installed Software — animated source breakdown for Pacman, AUR, and Flatpak with instant filtering and removal."
    }
  };

  const showcaseImg = document.getElementById("showcase-img");
  const showcaseCaption = document.getElementById("showcase-caption-text");
  const showcaseTabs = document.querySelectorAll("[data-shot]");

  showcaseTabs.forEach((btn) => {
    btn.addEventListener("click", () => {
      const key = btn.getAttribute("data-shot");
      const item = screenshots[key];
      if (!item || !showcaseImg) return;

      showcaseTabs.forEach((b) => {
        b.classList.remove("active");
        b.setAttribute("aria-selected", "false");
      });
      btn.classList.add("active");
      btn.setAttribute("aria-selected", "true");

      showcaseImg.src = item.src;
      showcaseImg.alt = item.alt;
      if (showcaseCaption) showcaseCaption.textContent = item.caption;
    });
  });

  // Lightbox
  const lightbox = document.getElementById("lightbox");
  const lightboxImg = document.getElementById("lightbox-img");
  const lightboxCaption = document.getElementById("lightbox-caption");
  const lightboxClose = document.getElementById("lightbox-close");
  const showcaseTrigger = document.getElementById("showcase-trigger");

  function openLightbox() {
    if (!lightbox || !lightboxImg || !showcaseImg) return;
    lightboxImg.src = showcaseImg.src;
    lightboxImg.alt = showcaseImg.alt;
    if (lightboxCaption && showcaseCaption) {
      lightboxCaption.textContent = showcaseCaption.textContent;
    }
    lightbox.classList.add("open");
    lightbox.setAttribute("aria-hidden", "false");
  }

  function closeLightbox() {
    if (!lightbox) return;
    lightbox.classList.remove("open");
    lightbox.setAttribute("aria-hidden", "true");
  }

  if (showcaseTrigger) {
    showcaseTrigger.addEventListener("click", openLightbox);
    showcaseTrigger.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openLightbox();
      }
    });
  }

  if (lightboxClose) lightboxClose.addEventListener("click", closeLightbox);
  if (lightbox) {
    lightbox.addEventListener("click", (e) => {
      if (e.target === lightbox) closeLightbox();
    });
  }
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeLightbox();
  });

  // --------------------------------------------------------------------------
  // Interactive Workbench Explorer
  // --------------------------------------------------------------------------
  const wbModules = {
    updates: {
      title: "System Updates & Health",
      badgeText: "Reviewed Workflow",
      badgeClass: "reviewed",
      lead:
        "Inspect live system health metrics and run reviewed official Arch upgrades with pre-flight safety checks, optional snapshots, and persistent logs.",
      points: [
        "Pre-flight lock, disk space, and repository checks",
        "Fresh checkupdates preview before launching sudo pacman -Syu",
        "Optional Timeshift or Snapper snapshot before upgrading",
        "Follows up with paru or yay (-Sua) for AUR packages",
        "Separate Flatpak update workflow & persistent private logs",
        "Live CPU, memory, GPU, swap, disk, and failed-unit viewers"
      ],
      docLabel: "Read System Management Guide",
      docUrl: "https://github.com/danny19862025/ArchEasy/blob/main/docs/system-management.md"
    },
    gaming: {
      title: "One-Click Gaming & Creator Studio",
      badgeText: "Reviewed Install",
      badgeClass: "reviewed",
      lead:
        "Get an x86_64 Arch Linux desktop ready for gaming or content creation with matching 64/32-bit Vulkan libraries, live logs, and graphical polkit auth.",
      points: [
        "Steam, Heroic, Lutris, MangoHud, GameMode, and Vulkan tools",
        "Explicit approval + backup for multilib repository changes",
        "Prefers official Arch packages with approved Flathub fallbacks",
        "Creator Studio for video, audio, 3D, and streaming workflows",
        "Stage cancellation, retry incomplete tools, and summary reports",
        "No third-party repos or hidden AUR helpers added automatically"
      ],
      docLabel: "Read Gaming Setup Guide",
      docUrl: "https://github.com/danny19862025/ArchEasy/blob/main/docs/gaming.md"
    },
    developer: {
      title: "One-Click Developer Tools",
      badgeText: "Reviewed Install",
      badgeClass: "reviewed",
      lead:
        "Bootstrap languages, editors, containers, databases, Linux GUI toolkits, and AI coding tools with customizable presets and environment checks.",
      points: [
        "Presets: Basic, Full Stack, Linux App, and AI Developer",
        "Environment check reports tool versions and missing deps",
        "Review exact operations and package sources before installing",
        "Expandable live logs with interactive prompt response field",
        "Retry only incomplete tools after an interrupted run",
        "Never silently edits shell profiles, user groups, or services"
      ],
      docLabel: "Read Developer Tools Guide",
      docUrl: "https://github.com/danny19862025/ArchEasy/blob/main/docs/developer-tools.md"
    },
    backups: {
      title: "Backup & Restore + Migration",
      badgeText: "Reviewed & Guided",
      badgeClass: "reviewed",
      lead:
        "Combine manual filesystem snapshots, encrypted Restic file backups, ZIP/Drive config archives, and portable setup-my-pc.sh exports.",
      points: [
        "Filesystem & snapshot provider inspection (Timeshift / Snapper)",
        "Encrypted Restic file backups to local-mounted repositories",
        "Export my system: generates portable setup-my-pc.sh script",
        "Config Timeline with dated versions and side-by-side diffs",
        "Automatic Fish syntax validation and transaction rollback",
        "Root restore & snapshot scheduling stay guided for safety"
      ],
      docLabel: "Read Backups & Recovery Guide",
      docUrl: "https://github.com/danny19862025/ArchEasy/blob/main/docs/backups.md"
    },
    hardware: {
      title: "Hardware, Drivers & Laptop",
      badgeText: "Read-Only + Reviewed",
      badgeClass: "readonly",
      lead:
        "Understand your running kernel, GPU PCI devices, NVIDIA DKMS headers, power profiles, and display backlight without blind auto-configuration.",
      points: [
        "Detects linux, linux-zen, linux-lts, and linux-hardened kernels",
        "Inspects GPU PCI info and distinguishes DKMS vs kernel drivers",
        "Verifies matching kernel headers for detected DKMS setups",
        "Laptop Optimisation: review existing power profiles & backlight",
        "Desktop Customisation: GNOME colour preference with revert",
        "Direct links to Arch Wiki driver guides; no silent boot edits"
      ],
      docLabel: "Read Workbench Capability Matrix",
      docUrl: "https://github.com/danny19862025/ArchEasy/blob/main/docs/workbench.md"
    },
    rescue: {
      title: "Disk, Network & System Rescue",
      badgeText: "Read-Only + Guided",
      badgeClass: "guided",
      lead:
        "Diagnose storage health, network/firewall posture, orphan packages, and Pacman cache bloat—keeping destructive repairs strictly guided and manual.",
      points: [
        "Disk Manager & SMART hardware inspection (with smartmontools)",
        "Network & Security read-only diagnostics for firewall & SSH",
        "Find orphan packages (pacman -Qdt) with re-checked removal",
        "Clean Pacman cache safely with paccache --remove --keep 3",
        "AppCleaner scans app caches & moves selected paths to Trash",
        "Boot/disk repair and hibernation remain guided, never risky"
      ],
      docLabel: "Read Workbench & Rescue Guide",
      docUrl: "https://github.com/danny19862025/ArchEasy/blob/main/docs/workbench.md"
    }
  };

  const wbButtons = document.querySelectorAll("[data-wb]");
  const wbTitle = document.getElementById("wb-title");
  const wbBadge = document.getElementById("wb-badge");
  const wbLead = document.getElementById("wb-lead");
  const wbPoints = document.getElementById("wb-points");
  const wbDocLink = document.getElementById("wb-doc-link");

  function renderWorkbench(key) {
    const mod = wbModules[key];
    if (!mod) return;

    wbButtons.forEach((btn) => {
      const isActive = btn.getAttribute("data-wb") === key;
      btn.classList.toggle("active", isActive);
      btn.setAttribute("aria-selected", String(isActive));
    });

    if (wbTitle) wbTitle.textContent = mod.title;
    if (wbBadge) {
      wbBadge.textContent = mod.badgeText;
      wbBadge.className = "safety-badge " + mod.badgeClass;
    }
    if (wbLead) wbLead.textContent = mod.lead;
    if (wbPoints) {
      wbPoints.innerHTML = mod.points.map((p) => `<li>${p}</li>`).join("");
    }
    if (wbDocLink) {
      wbDocLink.textContent = mod.docLabel + " →";
      wbDocLink.href = mod.docUrl;
    }
  }

  wbButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      renderWorkbench(btn.getAttribute("data-wb"));
    });
  });

  renderWorkbench("updates");

  // --------------------------------------------------------------------------
  // Install Tabs & Copy-to-Clipboard
  // --------------------------------------------------------------------------
  const installTabs = document.querySelectorAll("[data-install-tab]");
  const installPanels = document.querySelectorAll("[data-install-panel]");

  installTabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      const target = tab.getAttribute("data-install-tab");
      installTabs.forEach((t) => {
        t.classList.remove("active");
        t.setAttribute("aria-selected", "false");
      });
      installPanels.forEach((p) => {
        p.classList.toggle(
          "active",
          p.getAttribute("data-install-panel") === target
        );
      });
      tab.classList.add("active");
      tab.setAttribute("aria-selected", "true");
    });
  });

  document.querySelectorAll(".copy-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const block = btn.closest(".code-block");
      const code = block ? block.querySelector("pre")?.innerText : "";
      if (!code) return;
      try {
        await navigator.clipboard.writeText(code.trim());
        const prev = btn.textContent;
        btn.textContent = "Copied!";
        btn.classList.add("copied");
        setTimeout(() => {
          btn.textContent = prev;
          btn.classList.remove("copied");
        }, 1800);
      } catch {
        btn.textContent = "Select & copy";
      }
    });
  });
})();
