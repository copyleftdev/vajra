/* ================================================================
   VAJRA — GSAP Kinetic Animations
   Forged for the Agent Gods
   ================================================================ */

document.addEventListener("DOMContentLoaded", function () {
  // Only run on the index/landing page
  if (!document.getElementById("hero")) return;

  // Wait for GSAP to load
  if (typeof gsap === "undefined") {
    console.warn("GSAP not loaded — animations disabled");
    // Fallback: make everything visible
    document.querySelectorAll("[style*='opacity'],.vajra-logo,.vajra-title,.vajra-subtitle,.mantra-break,.mantra-preserve,.stat,.demo-card,.engine-card,.vajra-section,.vajra-footer").forEach(function(el) {
      el.style.opacity = "1";
      el.style.transform = "none";
    });
    return;
  }

  // Register ScrollTrigger if available
  if (typeof ScrollTrigger !== "undefined") {
    gsap.registerPlugin(ScrollTrigger);
  }

  /* ----------------------------------------------------------------
     HERO ENTRANCE — The Weapon Manifests
     ---------------------------------------------------------------- */

  var heroTl = gsap.timeline({ defaults: { ease: "power3.out" } });

  // Weapon fades in with a subtle scale pulse
  heroTl.fromTo("#vajra-logo",
    { opacity: 0, scale: 0.8, filter: "drop-shadow(0 0 0px rgba(196,163,90,0))" },
    { opacity: 1, scale: 1, filter: "drop-shadow(0 0 30px rgba(196,163,90,0.2))", duration: 1.8 }
  );

  // Title strikes in
  heroTl.fromTo("#main-title",
    { opacity: 0, letterSpacing: "3rem", filter: "blur(10px)" },
    { opacity: 1, letterSpacing: "1.2rem", filter: "blur(0px)", duration: 1.2 },
    "-=0.8"
  );

  // Subtitle slides up
  heroTl.fromTo("#subtitle",
    { opacity: 0, y: 20 },
    { opacity: 1, y: 0, duration: 0.8 },
    "-=0.4"
  );

  // Mantra — break, then preserve
  heroTl.fromTo("#mantra-break",
    { opacity: 0, x: -30 },
    { opacity: 1, x: 0, duration: 0.6 },
    "-=0.1"
  );

  heroTl.fromTo("#mantra-preserve",
    { opacity: 0, x: 30 },
    { opacity: 1, x: 0, duration: 0.6 },
    "-=0.3"
  );

  /* ----------------------------------------------------------------
     WEAPON IDLE — Subtle floating breath
     ---------------------------------------------------------------- */

  gsap.to("#vajra-logo", {
    y: -8,
    duration: 3,
    ease: "sine.inOut",
    yoyo: true,
    repeat: -1,
    delay: 2
  });

  // Subtle glow pulse on the weapon
  gsap.to("#vajra-logo", {
    filter: "drop-shadow(0 0 40px rgba(196,163,90,0.25))",
    duration: 2,
    ease: "sine.inOut",
    yoyo: true,
    repeat: -1,
    delay: 2.5
  });

  /* ----------------------------------------------------------------
     STATS BAR — Numbers count up
     ---------------------------------------------------------------- */

  var statsRevealed = false;

  function revealStats() {
    if (statsRevealed) return;
    statsRevealed = true;

    var stats = document.querySelectorAll(".stat");
    gsap.fromTo(stats,
      { opacity: 0, y: 20 },
      { opacity: 1, y: 0, duration: 0.6, stagger: 0.12, ease: "power2.out" }
    );

    // Count-up animation for numbers
    var numbers = [
      { el: document.querySelector("#stat-1 .stat-number"), target: 761 },
      { el: document.querySelector("#stat-2 .stat-number"), target: 12 },
      { el: document.querySelector("#stat-3 .stat-number"), target: 11 },
      // stat-4 is "22K" — handle separately
      { el: document.querySelector("#stat-5 .stat-number"), target: 0 }
    ];

    numbers.forEach(function(item) {
      if (!item.el) return;
      var obj = { val: 0 };
      gsap.to(obj, {
        val: item.target,
        duration: 1.5,
        delay: 0.3,
        ease: "power2.out",
        onUpdate: function() {
          item.el.textContent = Math.round(obj.val);
        }
      });
    });

    // Special handling for "22K"
    var stat4 = document.querySelector("#stat-4 .stat-number");
    if (stat4) {
      var kObj = { val: 0 };
      gsap.to(kObj, {
        val: 22,
        duration: 1.5,
        delay: 0.3,
        ease: "power2.out",
        onUpdate: function() {
          stat4.textContent = Math.round(kObj.val) + "K";
        }
      });
    }
  }

  // Trigger stats when scrolled into view, or after hero if no ScrollTrigger
  if (typeof ScrollTrigger !== "undefined") {
    ScrollTrigger.create({
      trigger: "#stats-bar",
      start: "top 80%",
      onEnter: revealStats
    });
  } else {
    setTimeout(revealStats, 2500);
  }

  /* ----------------------------------------------------------------
     DEMO CARDS — Staggered entrance
     ---------------------------------------------------------------- */

  function revealCards() {
    gsap.fromTo(".demo-card",
      { opacity: 0, y: 30, scale: 0.95 },
      { opacity: 1, y: 0, scale: 1, duration: 0.7, stagger: 0.1, ease: "power2.out" }
    );
  }

  if (typeof ScrollTrigger !== "undefined") {
    ScrollTrigger.create({
      trigger: "#demo-grid",
      start: "top 80%",
      onEnter: revealCards
    });
  } else {
    setTimeout(revealCards, 3000);
  }

  /* ----------------------------------------------------------------
     SECTIONS — Fade up on scroll
     ---------------------------------------------------------------- */

  var sections = document.querySelectorAll(".vajra-section");

  if (typeof ScrollTrigger !== "undefined") {
    sections.forEach(function(section) {
      gsap.fromTo(section,
        { opacity: 0, y: 40 },
        {
          opacity: 1, y: 0, duration: 1, ease: "power2.out",
          scrollTrigger: {
            trigger: section,
            start: "top 85%",
            toggleActions: "play none none none"
          }
        }
      );
    });
  } else {
    setTimeout(function() {
      gsap.to(sections, { opacity: 1, y: 0, duration: 0.8, stagger: 0.2 });
    }, 3500);
  }

  /* ----------------------------------------------------------------
     ENGINE CARDS — Cascade reveal
     ---------------------------------------------------------------- */

  function revealEngine() {
    gsap.fromTo(".engine-card",
      { opacity: 0, y: 20, borderColor: "rgba(107,90,42,0)" },
      {
        opacity: 1, y: 0,
        borderColor: "rgba(107,90,42,0.4)",
        duration: 0.6,
        stagger: 0.08,
        ease: "power2.out"
      }
    );
  }

  if (typeof ScrollTrigger !== "undefined") {
    ScrollTrigger.create({
      trigger: "#engine-grid",
      start: "top 80%",
      onEnter: revealEngine
    });
  } else {
    setTimeout(revealEngine, 4000);
  }

  /* ----------------------------------------------------------------
     FOOTER — Final mantra
     ---------------------------------------------------------------- */

  function revealFooter() {
    gsap.fromTo("#footer",
      { opacity: 0 },
      { opacity: 1, duration: 1.5, ease: "power1.out" }
    );
    gsap.fromTo(".footer-mantra",
      { letterSpacing: "0.6rem" },
      { letterSpacing: "0.3rem", duration: 2, ease: "power1.out" }
    );
  }

  if (typeof ScrollTrigger !== "undefined") {
    ScrollTrigger.create({
      trigger: "#footer",
      start: "top 90%",
      onEnter: revealFooter
    });
  } else {
    setTimeout(revealFooter, 5000);
  }

  /* ----------------------------------------------------------------
     GOLDEN PARTICLE FIELD — Ambient floating particles
     ---------------------------------------------------------------- */

  var canvas = document.createElement("canvas");
  canvas.id = "vajra-particles";
  document.body.appendChild(canvas);

  var ctx = canvas.getContext("2d");
  var particles = [];
  var particleCount = 40;

  function resizeCanvas() {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
  }
  resizeCanvas();
  window.addEventListener("resize", resizeCanvas);

  function createParticle() {
    return {
      x: Math.random() * canvas.width,
      y: Math.random() * canvas.height,
      size: Math.random() * 2 + 0.5,
      speedX: (Math.random() - 0.5) * 0.3,
      speedY: (Math.random() - 0.5) * 0.2 - 0.1,
      opacity: Math.random() * 0.4 + 0.1,
      pulse: Math.random() * Math.PI * 2
    };
  }

  for (var i = 0; i < particleCount; i++) {
    particles.push(createParticle());
  }

  function animateParticles() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    particles.forEach(function(p) {
      p.x += p.speedX;
      p.y += p.speedY;
      p.pulse += 0.02;

      // Wrap around
      if (p.x < 0) p.x = canvas.width;
      if (p.x > canvas.width) p.x = 0;
      if (p.y < 0) p.y = canvas.height;
      if (p.y > canvas.height) p.y = 0;

      var currentOpacity = p.opacity * (0.6 + 0.4 * Math.sin(p.pulse));

      ctx.beginPath();
      ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
      ctx.fillStyle = "rgba(196, 163, 90, " + currentOpacity + ")";
      ctx.fill();
    });

    requestAnimationFrame(animateParticles);
  }

  animateParticles();

  /* ----------------------------------------------------------------
     CODE BLOCK HOVER — Subtle gold glow on hover
     ---------------------------------------------------------------- */

  document.querySelectorAll("pre").forEach(function(pre) {
    pre.addEventListener("mouseenter", function() {
      gsap.to(pre, {
        borderColor: "rgba(196, 163, 90, 0.6)",
        boxShadow: "0 0 20px rgba(196, 163, 90, 0.08)",
        duration: 0.3
      });
    });
    pre.addEventListener("mouseleave", function() {
      gsap.to(pre, {
        borderColor: "rgba(107, 90, 42, 1)",
        boxShadow: "none",
        duration: 0.3
      });
    });
  });
});
