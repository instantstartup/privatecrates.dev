// The daily bars: one tab stop per component rather than 90. Arrow keys, Home and End move between the days that
// are shown (narrow screens hide the oldest 60). Without this script every bar is focusable, which still works.
(() => {
	const visible = (bar) => bar.offsetParent !== null;
	for (const list of document.querySelectorAll('.bars')) {
		const bars = [...list.querySelectorAll('.bar')];
		if (bars.length === 0) continue;
		const shown = () => bars.filter(visible);
		const focusOn = (target) => {
			for (const bar of bars) bar.tabIndex = bar === target ? 0 : -1;
			target.focus();
		};
		// Today (the last bar) is the entry point.
		for (const bar of bars) bar.tabIndex = -1;
		bars[bars.length - 1].tabIndex = 0;
		list.addEventListener('keydown', (event) => {
			const current = shown();
			const at = current.indexOf(document.activeElement);
			if (at < 0) return;
			let next;
			if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') next = current[Math.max(0, at - 1)];
			else if (event.key === 'ArrowRight' || event.key === 'ArrowDown') next = current[Math.min(current.length - 1, at + 1)];
			else if (event.key === 'Home') next = current[0];
			else if (event.key === 'End') next = current[current.length - 1];
			else return;
			event.preventDefault();
			focusOn(next);
		});
	}
})();
