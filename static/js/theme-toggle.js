/**
 * Theme Toggle System
 * Switches between Hasui Kawase (default) and OP-1 (alternate) themes
 */

class ThemeManager {
    constructor() {
        this.themes = {
            hasui: 'theme-hasui',
            op1: 'theme-op1'
        };
        this.currentTheme = this.loadTheme();
        this.init();
    }

    init() {
        // Apply saved theme on page load
        this.applyTheme(this.currentTheme);

        // Create theme toggle button
        this.createToggleButton();

        // Listen for keyboard shortcut (Ctrl/Cmd + T)
        document.addEventListener('keydown', (e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === 't') {
                e.preventDefault();
                this.toggleTheme();
            }
        });
    }

    loadTheme() {
        // Load from localStorage or default to hasui
        return localStorage.getItem('fermi-theme') || 'hasui';
    }

    saveTheme(theme) {
        localStorage.setItem('fermi-theme', theme);
    }

    applyTheme(theme) {
        const root = document.documentElement;
        const body = document.body;

        // Remove all theme classes
        Object.values(this.themes).forEach(themeClass => {
            root.classList.remove(themeClass);
            body.classList.remove(themeClass);
        });

        // Add new theme class
        const themeClass = this.themes[theme];
        root.classList.add(themeClass);
        body.classList.add(themeClass);

        this.currentTheme = theme;
        this.saveTheme(theme);

        // Update toggle button appearance
        this.updateToggleButton();

        // Dispatch event for other components
        window.dispatchEvent(new CustomEvent('themechange', {
            detail: { theme }
        }));
    }

    toggleTheme() {
        const newTheme = this.currentTheme === 'hasui' ? 'op1' : 'hasui';
        this.applyTheme(newTheme);

        // Animate the toggle
        this.animateToggle();
    }

    createToggleButton() {
        const button = document.createElement('button');
        button.className = 'theme-toggle';
        button.setAttribute('aria-label', 'Toggle theme');
        button.setAttribute('title', 'Switch theme (Ctrl+T)');

        // Create icon
        const icon = document.createElement('span');
        icon.className = 'theme-toggle-icon';
        button.appendChild(icon);

        // Add click handler
        button.addEventListener('click', () => this.toggleTheme());

        // Add to page
        document.body.appendChild(button);

        this.toggleButton = button;
        this.updateToggleButton();
    }

    updateToggleButton() {
        if (!this.toggleButton) return;

        const icon = this.toggleButton.querySelector('.theme-toggle-icon');

        if (this.currentTheme === 'hasui') {
            icon.textContent = '🎨';  // Art palette for Hasui theme
            this.toggleButton.setAttribute('title', 'Switch to OP-1 theme (Ctrl+T)');
        } else {
            icon.textContent = '🌊';  // Wave for Hasui theme
            this.toggleButton.setAttribute('title', 'Switch to Hasui theme (Ctrl+T)');
        }
    }

    animateToggle() {
        if (!this.toggleButton) return;

        this.toggleButton.style.transform = 'scale(0.9) rotate(180deg)';
        setTimeout(() => {
            this.toggleButton.style.transform = '';
        }, 200);
    }

    // Public API
    getTheme() {
        return this.currentTheme;
    }

    setTheme(theme) {
        if (this.themes[theme]) {
            this.applyTheme(theme);
        }
    }
}

// Initialize theme manager when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        window.themeManager = new ThemeManager();
    });
} else {
    window.themeManager = new ThemeManager();
}

// Export for module usage
if (typeof module !== 'undefined' && module.exports) {
    module.exports = ThemeManager;
}
