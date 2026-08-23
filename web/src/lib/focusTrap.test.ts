import { describe, it, expect, vi, afterEach } from 'vitest';
import { focusTrap } from './focusTrap';

function makeFocusable() {
	return { focus: vi.fn() } as unknown as HTMLElement;
}

function makeNode(focusables: HTMLElement[]) {
	const node = {
		focus: vi.fn(),
		querySelectorAll: vi.fn(() => focusables as unknown as NodeListOf<HTMLElement>),
		addEventListener: vi.fn(),
		removeEventListener: vi.fn(),
	} as unknown as HTMLElement;
	return node;
}

type KeydownHandler = (e: { key: string; shiftKey: boolean; preventDefault: () => void }) => void;

function keydownHandler(node: HTMLElement): KeydownHandler {
	return (node.addEventListener as unknown as ReturnType<typeof vi.fn>).mock.calls[0][1];
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('focusTrap', () => {
	it('focuses the node on setup', () => {
		vi.stubGlobal('document', { activeElement: null });
		const node = makeNode([]);
		focusTrap(node);
		expect(node.focus).toHaveBeenCalledTimes(1);
	});

	it('wraps a forward Tab from the last focusable to the first', () => {
		const first = makeFocusable();
		const last = makeFocusable();
		vi.stubGlobal('document', { activeElement: last });
		const node = makeNode([first, last]);
		focusTrap(node);
		const preventDefault = vi.fn();
		keydownHandler(node)({ key: 'Tab', shiftKey: false, preventDefault });
		expect(preventDefault).toHaveBeenCalledTimes(1);
		expect(first.focus).toHaveBeenCalledTimes(1);
		expect(last.focus).not.toHaveBeenCalled();
	});

	it('wraps a backward Shift+Tab from the first focusable to the last', () => {
		const first = makeFocusable();
		const last = makeFocusable();
		vi.stubGlobal('document', { activeElement: first });
		const node = makeNode([first, last]);
		focusTrap(node);
		const preventDefault = vi.fn();
		keydownHandler(node)({ key: 'Tab', shiftKey: true, preventDefault });
		expect(preventDefault).toHaveBeenCalledTimes(1);
		expect(last.focus).toHaveBeenCalledTimes(1);
		expect(first.focus).not.toHaveBeenCalled();
	});

	it('does not intercept a Tab when focus is between the first and last focusable', () => {
		const first = makeFocusable();
		const middle = makeFocusable();
		const last = makeFocusable();
		vi.stubGlobal('document', { activeElement: middle });
		const node = makeNode([first, middle, last]);
		focusTrap(node);
		const preventDefault = vi.fn();
		keydownHandler(node)({ key: 'Tab', shiftKey: false, preventDefault });
		expect(preventDefault).not.toHaveBeenCalled();
	});

	it('ignores non-Tab keys', () => {
		const first = makeFocusable();
		vi.stubGlobal('document', { activeElement: first });
		const node = makeNode([first]);
		focusTrap(node);
		const preventDefault = vi.fn();
		keydownHandler(node)({ key: 'Escape', shiftKey: false, preventDefault });
		expect(preventDefault).not.toHaveBeenCalled();
	});

	it('does nothing on Tab when there are no focusable elements', () => {
		vi.stubGlobal('document', { activeElement: null });
		const node = makeNode([]);
		focusTrap(node);
		const preventDefault = vi.fn();
		keydownHandler(node)({ key: 'Tab', shiftKey: false, preventDefault });
		expect(preventDefault).not.toHaveBeenCalled();
	});

	it('removes the keydown listener and restores previous focus on destroy', () => {
		const previouslyFocused = makeFocusable();
		vi.stubGlobal('document', { activeElement: previouslyFocused });
		const node = makeNode([]);
		const trap = focusTrap(node);
		trap.destroy();
		expect(node.removeEventListener).toHaveBeenCalledWith(
			'keydown',
			expect.any(Function)
		);
		expect(previouslyFocused.focus).toHaveBeenCalledTimes(1);
	});
});
