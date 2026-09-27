// Apply the visitor's saved theme choice ("light" or "dark") before first paint.
// With no saved choice, the CSS follows prefers-color-scheme.
try {
	var t = localStorage.getItem('pc-theme');
	if (t === 'light' || t === 'dark') document.documentElement.setAttribute('data-theme', t);
} catch {
	// Storage unavailable: follow the system setting.
}
