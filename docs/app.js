(() => {
  'use strict';

  /* ========================================================================
     Web Audio Subtle Click Feedback (Matches ArchEasy's optional click sound)
     ======================================================================== */
  let audioCtx = null;
  let soundEnabled = localStorage.getItem('archeasy-site-sound') !== 'false';
  const soundToggleBtn = document.getElementById('soundToggleBtn');

  function updateSoundBtn() {
    if (!soundToggleBtn) return;
    soundToggleBtn.setAttribute('aria-pressed', String(soundEnabled));
  }
  updateSoundBtn();

  function playClick(freq = 620) {
    if (!soundEnabled) return;
    try {
      if (!audioCtx) {
        const Ctx = window.AudioContext || window.webkitAudioContext;
        if (!Ctx) return;
        audioCtx = new Ctx();
      }
      if (audioCtx.state === 'suspended') audioCtx.resume();
      const osc = audioCtx.createOscillator();
      const gain = audioCtx.createGain();
      osc.type = 'sine';
      osc.frequency.setValueAtTime(freq, audioCtx.currentTime);
      osc.frequency.exponentialRampToValueAtTime(freq * 0.55, audioCtx.currentTime + 0.045);
      gain.gain.setValueAtTime(0.035, audioCtx.currentTime);
      gain.gain.exponentialRampToValueAtTime(0.0001, audioCtx.currentTime + 0.05);
      osc.connect(gain);
      gain.connect(audioCtx.destination);
      osc.start();
      osc.stop(audioCtx.currentTime + 0.055);
    } catch {
      // Ignore audio restrictions
    }
  }

  if (soundToggleBtn) {
    soundToggleBtn.addEventListener('click', () => {
      soundEnabled = !soundEnabled;
      localStorage.setItem('archeasy-site-sound', String(soundEnabled));
      updateSoundBtn();
      if (soundEnabled) playClick(740);
    });
  }

  /* ========================================================================
     Theme & Live Accent Customizer
     ======================================================================== */
  const root = document.documentElement;
  const themeToggle = document.getElementById('themeToggle');

  function applyTheme(theme) {
    root.setAttribute('data-theme', theme);
    localStorage.setItem('archeasy-site-theme', theme);
  }

  const savedTheme = localStorage.getItem('archeasy-site-theme');
  applyTheme(savedTheme === 'light' || savedTheme === 'dark' ? savedTheme : 'dark');

  function toggleTheme() {
    const next = root.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
    applyTheme(next);
    playClick(540);
  }

  if (themeToggle) themeToggle.addEventListener('click', toggleTheme);

  function hexToRgb(hex) {
    const clean = hex.replace('#', '');
    const num = parseInt(clean, 16);
    return `${(num >> 16) & 255}, ${(num >> 8) & 255}, ${num & 255}`;
  }

  const swatches = document.querySelectorAll('.swatch');
  swatches.forEach((sw) => {
    sw.addEventListener('click', () => {
      const c1 = sw.getAttribute('data-accent');
      const c2 = sw.getAttribute('data-accent-2');
      if (!c1 || !c2) return;
      root.style.setProperty('--accent', c1);
      root.style.setProperty('--accent-2', c2);
      root.style.setProperty('--accent-rgb', hexToRgb(c1));
      root.style.setProperty('--accent-2-rgb', hexToRgb(c2));
      swatches.forEach((s) => s.classList.toggle('active', s === sw));
      playClick(680);
    });
  });

  /* ========================================================================
     Slide Deck Engine + Auto-Slide Progress Bar
     ======================================================================== */
  const slides = Array.from(document.querySelectorAll('.slide'));
  const totalSlides = slides.length;
  let currentIndex = 0;

  const topbarSlideLabel = document.getElementById('topbarSlideLabel');
  const slideDotsContainer = document.getElementById('slideDots');
  const prevSlideBtn = document.getElementById('prevSlideBtn');
  const nextSlideBtn = document.getElementById('nextSlideBtn');
  const brandHomeBtn = document.getElementById('brandHomeBtn');
  const topbarInstallBtn = document.getElementById('topbarInstallBtn');
  const dockInstallBtn = document.getElementById('dockInstallBtn');
  const autoPlayToggleBtn = document.getElementById('autoPlayToggleBtn');
  const autoPlayLabel = document.getElementById('autoPlayLabel');
  const speedCycleBtn = document.getElementById('speedCycleBtn');
  const speedLabel = document.getElementById('speedLabel');
  const autoSlideFill = document.getElementById('autoSlideFill');

  const SPEEDS = [5000, 8000, 12000];
  let speedIdx = 1;
  let autoPlay = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  let autoStartTime = performance.now();
  let rafId = null;

  // Build slide dots
  const dotButtons = [];
  if (slideDotsContainer) {
    slides.forEach((slide, i) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'slide-dot' + (i === 0 ? ' active' : '');
      btn.setAttribute('role', 'tab');
      btn.setAttribute('aria-selected', String(i === 0));
      btn.title = `${String(i + 1).padStart(2, '0')} · ${slide.dataset.title || ''}`;
      btn.setAttribute('aria-label', `Go to slide ${i + 1}: ${slide.dataset.title || ''}`);
      btn.addEventListener('click', () => goToSlide(i));
      slideDotsContainer.appendChild(btn);
      dotButtons.push(btn);
    });
  }

  function updateHeaderAndDots() {
    const title = slides[currentIndex]?.dataset.title || '';
    const num = String(currentIndex + 1).padStart(2, '0');
    const tot = String(totalSlides).padStart(2, '0');
    if (topbarSlideLabel) {
      topbarSlideLabel.textContent = `${num} / ${tot} · ${title}`;
    }
    dotButtons.forEach((d, idx) => {
      const active = idx === currentIndex;
      d.classList.toggle('active', active);
      d.setAttribute('aria-selected', String(active));
    });
    updateOverviewCards();
  }

  function goToSlide(index, silent = false) {
    const next = ((index % totalSlides) + totalSlides) % totalSlides;
    if (next === currentIndex) {
      autoStartTime = performance.now();
      return;
    }

    slides.forEach((s, idx) => {
      s.classList.remove('active', 'prev');
      if (idx < next) s.classList.add('prev');
    });

    slides[next].classList.add('active');
    slides[next].scrollTop = 0;
    currentIndex = next;
    autoStartTime = performance.now();
    updateHeaderAndDots();
    if (!silent) playClick(580);
  }

  function nextSlide() {
    goToSlide(currentIndex + 1);
  }

  function prevSlide() {
    goToSlide(currentIndex - 1);
  }

  if (prevSlideBtn) prevSlideBtn.addEventListener('click', prevSlide);
  if (nextSlideBtn) nextSlideBtn.addEventListener('click', nextSlide);
  if (brandHomeBtn) brandHomeBtn.addEventListener('click', () => goToSlide(0));

  const installSlideIndex = totalSlides - 1;
  if (topbarInstallBtn) topbarInstallBtn.addEventListener('click', () => goToSlide(installSlideIndex));
  if (dockInstallBtn) dockInstallBtn.addEventListener('click', () => goToSlide(installSlideIndex));

  document.querySelectorAll('[data-goto-slide]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const target = Number(btn.getAttribute('data-goto-slide'));
      if (!Number.isNaN(target)) goToSlide(target);
    });
  });

  // Auto-slide loop
  function updateAutoPlayUI() {
    if (autoPlayToggleBtn) autoPlayToggleBtn.setAttribute('aria-pressed', String(autoPlay));
    if (autoPlayLabel) autoPlayLabel.textContent = autoPlay ? 'Auto' : 'Paused';
    if (!autoPlay && autoSlideFill) autoSlideFill.style.width = '0%';
  }

  function tickAutoSlide(now) {
    if (autoPlay && overviewModal?.hidden && lightboxModal?.hidden) {
      const duration = SPEEDS[speedIdx];
      const elapsed = now - autoStartTime;
      const pct = Math.min(100, (elapsed / duration) * 100);
      if (autoSlideFill) autoSlideFill.style.width = `${pct}%`;
      if (elapsed >= duration) {
        goToSlide(currentIndex + 1, true);
      }
    }
    rafId = requestAnimationFrame(tickAutoSlide);
  }

  if (autoPlayToggleBtn) {
    autoPlayToggleBtn.addEventListener('click', () => {
      autoPlay = !autoPlay;
      autoStartTime = performance.now();
      updateAutoPlayUI();
      playClick(640);
    });
  }

  if (speedCycleBtn) {
    speedCycleBtn.addEventListener('click', () => {
      speedIdx = (speedIdx + 1) % SPEEDS.length;
      if (speedLabel) speedLabel.textContent = `${SPEEDS[speedIdx] / 1000}s`;
      autoStartTime = performance.now();
      playClick(700);
    });
  }

  // Pause progress while pointer is interacting with install code or controls
  document.addEventListener('pointerdown', () => {
    autoStartTime = performance.now();
  });

  /* ========================================================================
     Slide Overview Modal (O) & Fullscreen (F)
     ======================================================================== */
  const overviewModal = document.getElementById('overviewModal');
  const overviewGrid = document.getElementById('overviewGrid');
  const overviewToggleBtn = document.getElementById('overviewToggleBtn');
  const overviewCloseBtn = document.getElementById('overviewCloseBtn');
  const overviewBackdrop = document.getElementById('overviewBackdrop');
  const fullscreenBtn = document.getElementById('fullscreenBtn');

  function buildOverview() {
    if (!overviewGrid) return;
    overviewGrid.innerHTML = '';
    slides.forEach((s, idx) => {
      const card = document.createElement('button');
      card.type = 'button';
      card.className = 'overview-card' + (idx === currentIndex ? ' active' : '');
      card.innerHTML = `<span>${String(idx + 1).padStart(2, '0')} / ${String(totalSlides).padStart(2, '0')}</span><strong>${s.dataset.title || ''}</strong>`;
      card.addEventListener('click', () => {
        goToSlide(idx);
        closeOverview();
      });
      overviewGrid.appendChild(card);
    });
  }

  function updateOverviewCards() {
    if (!overviewGrid) return;
    Array.from(overviewGrid.children).forEach((el, idx) => {
      el.classList.toggle('active', idx === currentIndex);
    });
  }

  function openOverview() {
    if (!overviewModal) return;
    overviewModal.hidden = false;
    playClick(600);
  }

  function closeOverview() {
    if (!overviewModal) return;
    overviewModal.hidden = true;
    autoStartTime = performance.now();
  }

  buildOverview();

  if (overviewToggleBtn) {
    overviewToggleBtn.addEventListener('click', () => {
      if (overviewModal?.hidden) openOverview();
      else closeOverview();
    });
  }
  if (overviewCloseBtn) overviewCloseBtn.addEventListener('click', closeOverview);
  if (overviewBackdrop) overviewBackdrop.addEventListener('click', closeOverview);

  function toggleFullscreen() {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen?.().catch(() => {});
    } else {
      document.exitFullscreen?.().catch(() => {});
    }
    playClick(660);
  }
  if (fullscreenBtn) fullscreenBtn.addEventListener('click', toggleFullscreen);

  /* ========================================================================
     Slide 1: Quick Download & Install Switcher on Main Slide
     ======================================================================== */
  const quickInstallData = {
    binary: {
      code: `sha256sum -c archeasy-0.1.2-x86_64-linux.tar.gz.sha256 && tar -xzf archeasy-0.1.2-x86_64-linux.tar.gz\ncd archeasy-0.1.2-x86_64-linux && install -Dm755 archeasy "$HOME/.local/bin/archeasy" && bash scripts/install-desktop.sh`,
      hint: 'Download tarball + <code>.sha256</code> from GitHub Releases &middot; Requires GTK4 4.10+ &amp; Libadwaita 1.2+'
    },
    cargo: {
      code: `git clone https://github.com/danny19862025/ArchEasy.git && cd ArchEasy\ncargo install --path . --locked && bash scripts/install-desktop.sh`,
      hint: 'Builds release binary with Rust &amp; registers the desktop launcher under <code>~/.local/share</code>'
    },
    pkgbuild: {
      code: `git clone https://github.com/danny19862025/ArchEasy.git && mkdir -p /tmp/archeasy-package\ncp ArchEasy/scripts/PKGBUILD /tmp/archeasy-package/ && cd /tmp/archeasy-package && makepkg -si`,
      hint: 'Builds &amp; installs a native Arch Linux package cleanly through Pacman'
    }
  };

  const heroInstallCode = document.getElementById('heroInstallCode');
  const heroInstallHint = document.getElementById('heroInstallHint');
  const heroCopyBtn = document.getElementById('heroCopyBtn');
  const quickTabs = document.querySelectorAll('[data-quick-install]');

  quickTabs.forEach((tab) => {
    tab.addEventListener('click', () => {
      const key = tab.getAttribute('data-quick-install');
      const data = quickInstallData[key];
      if (!data) return;
      quickTabs.forEach((t) => {
        const active = t === tab;
        t.classList.toggle('active', active);
        t.setAttribute('aria-selected', String(active));
      });
      if (heroInstallCode) heroInstallCode.querySelector('code').textContent = data.code;
      if (heroInstallHint) heroInstallHint.innerHTML = data.hint;
      playClick(620);
    });
  });

  if (heroCopyBtn && heroInstallCode) {
    heroCopyBtn.addEventListener('click', () => copyText(heroInstallCode.innerText, heroCopyBtn));
  }

  /* ========================================================================
     Slide 2: Interactive Live Task Search Preview
     ======================================================================== */
  const demoQueries = {
    video: [
      { name: 'kdenlive', desc: 'Non-linear video editor (KDE)', src: 'Pacman' },
      { name: 'shotcut', desc: 'Cross-platform video editor', src: 'Flatpak' },
      { name: 'davinci-resolve', desc: 'Professional editing & colour grading', src: 'AUR' }
    ],
    gaming: [
      { name: 'steam', desc: 'Valve digital storefront & Proton runtime', src: 'Pacman' },
      { name: 'heroic-games-launcher-bin', desc: 'Epic, GOG & Amazon launcher', src: 'AUR' },
      { name: 'mangohud', desc: 'Vulkan & OpenGL performance overlay', src: 'Pacman' }
    ],
    terminal: [
      { name: 'ghostty', desc: 'Fast GPU-accelerated native terminal', src: 'Pacman' },
      { name: 'starship', desc: 'Minimal, blazing-fast shell prompt', src: 'Pacman' },
      { name: 'danny19862025/ArchEasy', desc: 'Root PKGBUILD & verified release', src: 'GitHub' }
    ]
  };

  const demoResults = document.getElementById('demoResults');
  const demoChips = document.querySelectorAll('[data-demo-query]');

  function renderDemoQuery(key) {
    const items = demoQueries[key];
    if (!items || !demoResults) return;
    demoResults.innerHTML = items
      .map(
        (item) => `
        <div class="demo-item">
          <div>
            <strong>${item.name}</strong>
            <small>${item.desc}</small>
          </div>
          <span class="demo-src-pill">${item.src}</span>
        </div>`
      )
      .join('');
  }

  demoChips.forEach((chip) => {
    chip.addEventListener('click', () => {
      demoChips.forEach((c) => c.classList.toggle('active', c === chip));
      renderDemoQuery(chip.getAttribute('data-demo-query'));
      playClick(640);
    });
  });
  renderDemoQuery('video');

  /* ========================================================================
     Slide 3: Screenshot Gallery & Lightbox
     ======================================================================== */
  const screenshots = {
    software: {
      src: 'assets/software-centre.png',
      alt: 'ArchEasy Software Centre with integrated workbench sidebar and multi-source search',
      caption: 'Software Centre — unified Pacman, AUR, Flatpak, and GitHub search with the always-visible workbench sidebar.'
    },
    configs: {
      src: 'assets/configs-workbench.png',
      alt: 'ArchEasy Configs page with built-in presets, dry-run preview, and backup controls',
      caption: 'Configs & Presets — preview changes before applying, back up to ZIP or Google Drive, and browse Config Timeline diffs.'
    },
    installed: {
      src: 'assets/installed.png',
      alt: 'ArchEasy Installed tab showing animated percentage circles for Pacman, AUR, and Flatpak',
      caption: 'Installed Software — animated source breakdown for Pacman, AUR, and Flatpak with instant filtering and removal.'
    }
  };

  const showcaseImg = document.getElementById('showcaseImg');
  const showcaseCaptionText = document.getElementById('showcaseCaptionText');
  const showcaseTabs = document.querySelectorAll('[data-shot]');
  const showcaseTrigger = document.getElementById('showcaseTrigger');

  showcaseTabs.forEach((btn) => {
    btn.addEventListener('click', () => {
      const item = screenshots[btn.getAttribute('data-shot')];
      if (!item || !showcaseImg) return;
      showcaseTabs.forEach((b) => {
        const active = b === btn;
        b.classList.toggle('active', active);
        b.setAttribute('aria-selected', String(active));
      });
      showcaseImg.src = item.src;
      showcaseImg.alt = item.alt;
      if (showcaseCaptionText) showcaseCaptionText.textContent = item.caption;
      playClick(600);
    });
  });

  const lightboxModal = document.getElementById('lightboxModal');
  const lightboxImg = document.getElementById('lightboxImg');
  const lightboxCaption = document.getElementById('lightboxCaption');
  const lightboxCloseBtn = document.getElementById('lightboxCloseBtn');
  const lightboxBackdrop = document.getElementById('lightboxBackdrop');

  function openLightbox() {
    if (!lightboxModal || !lightboxImg || !showcaseImg) return;
    lightboxImg.src = showcaseImg.src;
    lightboxImg.alt = showcaseImg.alt;
    if (lightboxCaption && showcaseCaptionText) {
      lightboxCaption.textContent = showcaseCaptionText.textContent;
    }
    lightboxModal.hidden = false;
    playClick(650);
  }

  function closeLightbox() {
    if (!lightboxModal) return;
    lightboxModal.hidden = true;
    autoStartTime = performance.now();
  }

  if (showcaseTrigger) {
    showcaseTrigger.addEventListener('click', openLightbox);
    showcaseTrigger.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        openLightbox();
      }
    });
  }
  if (lightboxCloseBtn) lightboxCloseBtn.addEventListener('click', closeLightbox);
  if (lightboxBackdrop) lightboxBackdrop.addEventListener('click', closeLightbox);

  /* ========================================================================
     Slide 4: Interactive Workbench Explorer
     ======================================================================== */
  const wbModules = {
    updates: {
      title: 'System Updates & Health',
      badgeText: 'Reviewed Workflow',
      badgeClass: 'reviewed',
      lead: 'Inspect live system health metrics and run reviewed official Arch upgrades with pre-flight safety checks, optional snapshots, and persistent logs.',
      points: [
        'Pre-flight lock, disk space, and repository checks',
        'Fresh checkupdates preview before launching sudo pacman -Syu',
        'Optional Timeshift or Snapper snapshot before upgrading',
        'Follows up with paru or yay (-Sua) for AUR packages',
        'Separate Flatpak update workflow & persistent private logs',
        'Live CPU, memory, GPU, swap, disk, and failed-unit viewers'
      ],
      docLabel: 'Read System Management Guide',
      docUrl: 'https://github.com/danny19862025/ArchEasy/blob/main/docs/system-management.md'
    },
    gaming: {
      title: 'One-Click Gaming & Creator Studio',
      badgeText: 'Reviewed Install',
      badgeClass: 'reviewed',
      lead: 'Get an x86_64 Arch Linux desktop ready for gaming or content creation with matching 64/32-bit Vulkan libraries, live logs, and graphical polkit auth.',
      points: [
        'Steam, Heroic, Lutris, MangoHud, GameMode, and Vulkan tools',
        'Explicit approval + backup for multilib repository changes',
        'Prefers official Arch packages with approved Flathub fallbacks',
        'Creator Studio for video, audio, 3D, and streaming workflows',
        'Stage cancellation, retry incomplete tools, and summary reports',
        'No third-party repos or hidden AUR helpers added automatically'
      ],
      docLabel: 'Read Gaming Setup Guide',
      docUrl: 'https://github.com/danny19862025/ArchEasy/blob/main/docs/gaming.md'
    },
    developer: {
      title: 'One-Click Developer Tools',
      badgeText: 'Reviewed Install',
      badgeClass: 'reviewed',
      lead: 'Bootstrap languages, editors, containers, databases, Linux GUI toolkits, and AI coding tools with customizable presets and environment checks.',
      points: [
        'Presets: Basic, Full Stack, Linux App, and AI Developer',
        'Environment check reports tool versions and missing deps',
        'Review exact operations and package sources before installing',
        'Expandable live logs with interactive prompt response field',
        'Retry only incomplete tools after an interrupted run',
        'Never silently edits shell profiles, user groups, or services'
      ],
      docLabel: 'Read Developer Tools Guide',
      docUrl: 'https://github.com/danny19862025/ArchEasy/blob/main/docs/developer-tools.md'
    },
    backups: {
      title: 'Backup & Restore + Migration',
      badgeText: 'Reviewed & Guided',
      badgeClass: 'reviewed',
      lead: 'Combine manual filesystem snapshots, encrypted Restic file backups, ZIP/Drive config archives, and portable setup-my-pc.sh exports.',
      points: [
        'Filesystem & snapshot provider inspection (Timeshift / Snapper)',
        'Encrypted Restic file backups to local-mounted repositories',
        'Export my system: generates portable setup-my-pc.sh script',
        'Config Timeline with dated versions and side-by-side diffs',
        'Automatic Fish syntax validation and transaction rollback',
        'Root restore & snapshot scheduling stay guided for safety'
      ],
      docLabel: 'Read Backups & Recovery Guide',
      docUrl: 'https://github.com/danny19862025/ArchEasy/blob/main/docs/backups.md'
    },
    hardware: {
      title: 'Hardware, Drivers & Laptop',
      badgeText: 'Read-Only + Reviewed',
      badgeClass: 'readonly',
      lead: 'Understand your running kernel, GPU PCI devices, NVIDIA DKMS headers, power profiles, and display backlight without blind auto-configuration.',
      points: [
        'Detects linux, linux-zen, linux-lts, and linux-hardened kernels',
        'Inspects GPU PCI info and distinguishes DKMS vs kernel drivers',
        'Verifies matching kernel headers for detected DKMS setups',
        'Laptop Optimisation: review existing power profiles & backlight',
        'Desktop Customisation: GNOME colour preference with revert',
        'Direct links to Arch Wiki driver guides; no silent boot edits'
      ],
      docLabel: 'Read Workbench Capability Matrix',
      docUrl: 'https://github.com/danny19862025/ArchEasy/blob/main/docs/workbench.md'
    },
    rescue: {
      title: 'Disk, Network & System Rescue',
      badgeText: 'Read-Only + Guided',
      badgeClass: 'guided',
      lead: 'Diagnose storage health, network/firewall posture, orphan packages, and Pacman cache bloat—keeping destructive repairs strictly guided and manual.',
      points: [
        'Disk Manager & SMART hardware inspection (with smartmontools)',
        'Network & Security read-only diagnostics for firewall & SSH',
        'Find orphan packages (pacman -Qdt) with re-checked removal',
        'Clean Pacman cache safely with paccache --remove --keep 3',
        'AppCleaner scans app caches & moves selected paths to Trash',
        'Boot/disk repair and hibernation remain guided, never risky'
      ],
      docLabel: 'Read Workbench & Rescue Guide',
      docUrl: 'https://github.com/danny19862025/ArchEasy/blob/main/docs/workbench.md'
    }
  };

  const wbButtons = document.querySelectorAll('[data-wb]');
  const wbTitle = document.getElementById('wbTitle');
  const wbBadge = document.getElementById('wbBadge');
  const wbLead = document.getElementById('wbLead');
  const wbPoints = document.getElementById('wbPoints');
  const wbDocLink = document.getElementById('wbDocLink');

  function renderWorkbench(key) {
    const mod = wbModules[key];
    if (!mod) return;
    wbButtons.forEach((btn) => {
      const active = btn.getAttribute('data-wb') === key;
      btn.classList.toggle('active', active);
      btn.setAttribute('aria-selected', String(active));
    });
    if (wbTitle) wbTitle.textContent = mod.title;
    if (wbBadge) {
      wbBadge.textContent = mod.badgeText;
      wbBadge.className = 'safety-badge ' + mod.badgeClass;
    }
    if (wbLead) wbLead.textContent = mod.lead;
    if (wbPoints) wbPoints.innerHTML = mod.points.map((p) => `<li>${p}</li>`).join('');
    if (wbDocLink) {
      wbDocLink.textContent = mod.docLabel + ' →';
      wbDocLink.href = mod.docUrl;
    }
  }

  wbButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      renderWorkbench(btn.getAttribute('data-wb'));
      playClick(600);
    });
  });
  renderWorkbench('updates');

  /* ========================================================================
     Slide 7: Install Tabs & Copy Buttons
     ======================================================================== */
  const installTabs = document.querySelectorAll('[data-install-tab]');
  const installPanels = document.querySelectorAll('[data-install-panel]');

  installTabs.forEach((tab) => {
    tab.addEventListener('click', () => {
      const target = tab.getAttribute('data-install-tab');
      installTabs.forEach((t) => {
        const active = t === tab;
        t.classList.toggle('active', active);
        t.setAttribute('aria-selected', String(active));
      });
      installPanels.forEach((p) => {
        p.classList.toggle('active', p.getAttribute('data-install-panel') === target);
      });
      playClick(620);
    });
  });

  async function copyText(text, btn) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text.trim());
      const prev = btn.textContent;
      btn.textContent = 'Copied!';
      btn.classList.add('copied');
      playClick(780);
      setTimeout(() => {
        btn.textContent = prev;
        btn.classList.remove('copied');
      }, 1700);
    } catch {
      btn.textContent = 'Select & copy';
    }
  }

  document.querySelectorAll('.code-block .copy-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const pre = btn.closest('.code-block')?.querySelector('pre');
      if (pre) copyText(pre.innerText, btn);
    });
  });

  /* ========================================================================
     Keyboard & Touch Swipe Navigation
     ======================================================================== */
  document.addEventListener('keydown', (e) => {
    if (e.target.closest('input, textarea, select')) return;

    if (e.key === 'Escape') {
      if (!lightboxModal?.hidden) closeLightbox();
      else if (!overviewModal?.hidden) closeOverview();
      return;
    }

    if (!lightboxModal?.hidden) return;

    if (e.key === 'o' || e.key === 'O') {
      e.preventDefault();
      if (overviewModal?.hidden) openOverview();
      else closeOverview();
      return;
    }

    if (!overviewModal?.hidden) return;

    if (e.key === 'ArrowRight' || e.key === 'PageDown') {
      e.preventDefault();
      nextSlide();
    } else if (e.key === 'ArrowLeft' || e.key === 'PageUp') {
      e.preventDefault();
      prevSlide();
    } else if (e.key === ' ') {
      if (!e.target.closest('button, a')) {
        e.preventDefault();
        autoPlay = !autoPlay;
        autoStartTime = performance.now();
        updateAutoPlayUI();
      }
    } else if (e.key === 't' || e.key === 'T') {
      toggleTheme();
    } else if (e.key === 'f' || e.key === 'F') {
      toggleFullscreen();
    } else if (e.key === 'i' || e.key === 'I') {
      goToSlide(installSlideIndex);
    } else if (/^[1-9]$/.test(e.key)) {
      const idx = Number(e.key) - 1;
      if (idx < totalSlides) goToSlide(idx);
    }
  });

  let touchStartX = 0;
  let touchStartY = 0;
  document.addEventListener(
    'touchstart',
    (e) => {
      touchStartX = e.changedTouches[0].clientX;
      touchStartY = e.changedTouches[0].clientY;
    },
    { passive: true }
  );
  document.addEventListener(
    'touchend',
    (e) => {
      const dx = e.changedTouches[0].clientX - touchStartX;
      const dy = e.changedTouches[0].clientY - touchStartY;
      if (Math.abs(dx) > 55 && Math.abs(dx) > Math.abs(dy) * 1.4) {
        if (dx < 0) nextSlide();
        else prevSlide();
      }
    },
    { passive: true }
  );

  // Initialize
  updateHeaderAndDots();
  updateAutoPlayUI();
  rafId = requestAnimationFrame(tickAutoSlide);
})();
