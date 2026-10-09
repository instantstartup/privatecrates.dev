/**
 * Every illustration renders its own <svg> with a fixed viewBox, so it can stand alone (sized with CSS through
 * `class`) or be nested inside a larger scene by passing x, y, width and height in the parent's user units.
 */
export interface IllustrationProps {
	class?: string;
	x?: number;
	y?: number;
	width?: number | string;
	height?: number | string;
	/** Accessible name. Omit for decorative use: the SVG is then hidden from assistive technology. */
	title?: string;
}

export type Tone = 'primary' | 'accent' | 'muted' | 'raised';
