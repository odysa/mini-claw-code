(function () {
    const EN_BASE = '/mini-claw-code/';
    const ZH_BASE = '/mini-claw-code/zh/';

    const lang = (document.documentElement.lang || 'en').toLowerCase();
    const isZh = lang.startsWith('zh');

    function computeToggleUrl() {
        const path = window.location.pathname;

        // Production paths under /mini-claw-code/ (and /mini-claw-code/zh/).
        if (isZh && path.startsWith(ZH_BASE)) {
            return EN_BASE + path.slice(ZH_BASE.length);
        }
        if (!isZh && path.startsWith(EN_BASE)) {
            const rest = path.slice(EN_BASE.length);
            if (!rest.startsWith('zh/')) {
                return ZH_BASE + rest;
            }
        }

        // Local dev — combined site served at `/` with `/zh/` subdir.
        if (isZh && path.startsWith('/zh/')) {
            // '/zh/ch01.html' → '/ch01.html'
            return path.slice(3);
        }
        if (!isZh && !path.startsWith('/zh/')) {
            // '/ch01.html' → '/zh/ch01.html'
            return '/zh' + path;
        }

        return null;
    }

    function createButton() {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.id = 'language-toggle';
        btn.className = 'icon-button';
        btn.setAttribute('aria-label', isZh ? 'Switch to English' : '切换到中文');
        btn.title = isZh ? 'Switch to English' : '切换到中文';
        btn.textContent = isZh ? 'EN' : '中文';
        btn.style.cssText = 'width: auto; padding: 0 10px; font-size: 0.95em; font-weight: 600;';
        btn.addEventListener('click', () => {
            const url = computeToggleUrl();
            if (url) {
                window.location.href = url;
                return;
            }
            // Last-resort fallback: hop to the sibling book's homepage.
            window.location.href = isZh ? '/' : '/zh/';
        });
        return btn;
    }

    function inject() {
        const rightButtons = document.querySelector('.right-buttons');
        if (!rightButtons) {
            setTimeout(inject, 50);
            return;
        }
        if (document.getElementById('language-toggle')) return;
        rightButtons.insertBefore(createButton(), rightButtons.firstChild);
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', inject);
    } else {
        inject();
    }
})();
