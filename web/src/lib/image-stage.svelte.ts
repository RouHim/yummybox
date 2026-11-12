import { ApiError, loadImageFromUrl } from './api';
import { t } from './i18n';
import type { TranslationKey } from './i18n/types';

/**
 * Browser image capture shared by the image pickers: drag-and-drop, paste
 * (including the per-browser fallbacks), clipboard read, URL load and the
 * validation/error policy.
 *
 * A picker supplies its policy and renders the staged files; it does not
 * re-implement how a file gets staged.
 */

/** Files whose MIME type or extension marks them as images. The extension
 * list mirrors what the backend can decode (jpeg, png, webp, gif, bmp);
 * formats it cannot read are rejected here instead of failing the upload. */
export function looksLikeImage(file: File): boolean {
	if (file.type.startsWith('image/')) return true;
	const ext = file.name.split('.').pop()?.toLowerCase();
	return ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp'].includes(ext ?? '');
}

export interface ImageCapturePolicy {
	/** Reject one file before staging; returns the message key. */
	validateFile?: (file: File) => TranslationKey | null;
	/** Reject the resulting set; returns the message key. */
	validateTotal?: (files: File[]) => TranslationKey | null;
	/**
	 * Maximum staged files. `1` means a new file replaces the current one;
	 * larger values mean an overflowing add is rejected with `tooManyMessage`.
	 */
	maxFiles: number;
	/** Message shown when a multi-file surface would overflow. */
	tooManyMessage?: TranslationKey;
	/** Treat a plain-text drag as a URL (single-image surface). */
	allowPlainTextDrag?: boolean;
	/** Leave pastes into inputs, textareas and contenteditable to the browser. */
	ignoreEditablePaste?: boolean;
}

export interface ImageCaptureCallbacks {
	/** The staged set changed. */
	onchange: (files: File[]) => void;
	/** A message to show, or `null` to clear it. */
	onerror: (message: string | null) => void;
}

/** Error codes the image-URL endpoint answers with. */
const IMAGE_URL_ERROR_KEYS: Record<string, TranslationKey> = {
	image_url_unreachable: 'imageErrorUrlUnreachable',
	image_url_not_image: 'imageErrorUrlNotImage',
	image_url_too_large: 'imageErrorUrlTooLarge',
};

/** Map a failed URL load to a user-facing message. */
export function imageUrlErrorMessage(err: unknown): string {
	if (err instanceof ApiError && err.code) {
		const key = IMAGE_URL_ERROR_KEYS[err.code];
		if (key) return t(key);
	}
	return t('imageErrorUrlGeneric');
}

export class ImageCapture {
	files = $state<File[]>([]);
	error = $state<string | null>(null);
	urlInput = $state('');
	urlLoading = $state(false);
	urlError = $state<string | null>(null);
	urlRowOpen = $state(false);
	dragDepth = $state(0);
	isDragging = $derived(this.dragDepth > 0);

	#policy: ImageCapturePolicy;
	#callbacks: ImageCaptureCallbacks;

	constructor(
		policy: ImageCapturePolicy,
		callbacks: ImageCaptureCallbacks,
		initialFiles: File[] = []
	) {
		this.#policy = policy;
		this.#callbacks = callbacks;
		this.files = initialFiles;
	}

	/** Stage files; the whole add is rejected when any file or the set is invalid. */
	add(incoming: File[]): void {
		if (incoming.length === 0) return;
		this.urlError = null;

		const files = this.#policy.maxFiles === 1 ? incoming.slice(0, 1) : incoming;
		for (const file of files) {
			const rejection = this.#policy.validateFile?.(file);
			if (rejection) {
				this.#fail(rejection);
				return;
			}
		}

		const next = this.#policy.maxFiles === 1 ? files : [...this.files, ...files];
		if (next.length > this.#policy.maxFiles) {
			if (this.#policy.tooManyMessage) this.#fail(this.#policy.tooManyMessage);
			return;
		}
		const totalRejection = this.#policy.validateTotal?.(next);
		if (totalRejection) {
			this.#fail(totalRejection);
			return;
		}

		this.files = next;
		this.#clearMessages();
		this.#notify();
	}

	remove(index: number): void {
		this.files = this.files.filter((_, i) => i !== index);
		this.#clearMessages();
		this.#notify();
	}

	/** Drop the staged files without notifying; the caller owns the outcome. */
	reset(): void {
		this.files = [];
		this.#clearMessages();
	}

	/** Mirror an externally owned file list without notifying back. */
	replace(files: File[]): void {
		this.files = [...files];
	}

	// --- DnD handlers (whole component surface) ---

	onDragEnter(e: DragEvent): void {
		if (this.#isFileDrag(e.dataTransfer)) this.dragDepth++;
		e.preventDefault();
	}

	onDragOver(e: DragEvent): void {
		e.preventDefault();
	}

	onDragLeave(): void {
		this.dragDepth = Math.max(0, this.dragDepth - 1);
	}

	onDrop(e: DragEvent): void {
		e.preventDefault();
		this.dragDepth = 0;
		const dropped = e.dataTransfer?.files;
		if (dropped && dropped.length > 0) {
			this.add(Array.from(dropped));
			return;
		}
		// Cross-window browser drag: payload is a URL string, not a File.
		const url = this.#readDraggedUrl(e.dataTransfer);
		if (url) void this.#stageFromUrl(url, 'drop');
	}

	// --- Upload tile (click-to-browse) ---

	onFileInputChange(e: Event): void {
		const target = e.target as HTMLInputElement;
		this.add(Array.from(target.files ?? []));
		target.value = '';
	}

	// --- Paste tile ---

	onPaste(e: ClipboardEvent): void {
		if (this.#policy.ignoreEditablePaste && isEditableTarget(e.target)) return;
		const clipboard = e.clipboardData;
		if (!clipboard) return;

		// Chrome/Edge populate clipboardData.files for image pastes.
		if (clipboard.files.length > 0) {
			const imageFile = Array.from(clipboard.files).find((file) =>
				file.type.startsWith('image/')
			);
			if (imageFile) {
				e.preventDefault();
				this.add([imageFile]);
				return;
			}
		}

		// Firefox (Linux/macOS): clipboardData.files is empty; DataTransferItemList
		// has image items with kind='file'.
		for (const item of Array.from(clipboard.items)) {
			if (item.kind === 'file' && item.type.startsWith('image/')) {
				const file = item.getAsFile();
				if (file) {
					e.preventDefault();
					this.add([file]);
					return;
				}
			}
		}

		// Firefox (Windows): OS screenshots arrive as text/html with an embedded
		// <img> tag containing a data: URI (e.g. Win+Shift+S).
		const html = clipboard.getData('text/html');
		if (!html) return;
		const doc = new DOMParser().parseFromString(html, 'text/html');
		const image = doc.querySelector('img');
		if (!image?.src) return;
		const src = image.src;

		if (src.startsWith('data:')) {
			e.preventDefault();
			const comma = src.indexOf(',');
			if (comma === -1) return;
			const mime = src.slice(0, comma).split(':')[1]?.split(';')[0] || 'image/png';
			try {
				const binary = atob(src.slice(comma + 1));
				const bytes = new Uint8Array(binary.length);
				for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
				this.add([new File([bytes], 'pasted.png', { type: mime })]);
			} catch {
				// Invalid base64 — ignore.
			}
			return;
		}

		if (src.startsWith('blob:')) {
			e.preventDefault();
			fetch(src)
				.then((response) => response.blob())
				.then((blob) => {
					this.add([new File([blob], 'pasted.png', { type: blob.type || 'image/png' })]);
				})
				.catch(() => {});
		}
	}

	/** Read the system clipboard via the async Clipboard API. */
	async onPasteClick(): Promise<void> {
		try {
			const clipboardItems = await navigator.clipboard.read();
			for (const item of clipboardItems) {
				for (const type of item.types) {
					if (type.startsWith('image/')) {
						const blob = await item.getType(type);
						this.add([new File([blob], 'pasted.png', { type })]);
						return;
					}
				}
			}
		} catch {
			// Clipboard read not supported or denied — silent no-op.
		}
	}

	onPasteKeyDown(e: KeyboardEvent): void {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			void this.onPasteClick();
		}
	}

	// --- URL load ---

	async loadUrl(): Promise<void> {
		this.urlError = null;
		const url = this.urlInput.trim();
		if (!url) return;
		this.urlLoading = true;
		try {
			await this.#stageFromUrl(url, 'url');
			if (!this.urlError) {
				this.urlInput = '';
				this.urlRowOpen = false;
			}
		} finally {
			this.urlLoading = false;
		}
	}

	toggleUrlRow(): void {
		this.urlRowOpen = !this.urlRowOpen;
		this.urlError = null;
	}

	// --- internals ---

	async #stageFromUrl(url: string, target: 'url' | 'drop'): Promise<void> {
		try {
			const response = await loadImageFromUrl(url);
			const bytes = Uint8Array.from(atob(response.imageBase64), (char) => char.charCodeAt(0));
			this.add([new File([bytes], 'imported.jpg', { type: 'image/jpeg' })]);
		} catch (err) {
			const message = imageUrlErrorMessage(err);
			if (target === 'url') {
				this.urlError = message;
			} else {
				this.#failMessage(message);
			}
		}
	}

	#fail(key: TranslationKey): void {
		this.#failMessage(t(key));
	}

	#failMessage(message: string): void {
		this.error = message;
		this.#callbacks.onerror(message);
	}

	#clearMessages(): void {
		this.error = null;
		this.#callbacks.onerror(null);
	}

	#notify(): void {
		this.#callbacks.onchange([...this.files]);
	}

	#isFileDrag(dt: DataTransfer | null): boolean {
		if (!dt) return false;
		// Browsers protect the file list during dragenter/dragover, so it can
		// be empty even when the user is dragging files. The `types` list is
		// the reliable signal.
		if (dt.files.length > 0) return true;
		const types = Array.from(dt.types);
		if (types.includes('Files')) return true;
		if (types.includes('text/uri-list')) return true;
		// Dragging arbitrary selected text exposes text/plain; only the
		// single-image surface treats it as a URL candidate.
		return this.#policy.allowPlainTextDrag === true && types.includes('text/plain');
	}

	#readDraggedUrl(dt: DataTransfer | null): string | null {
		if (!dt) return null;
		// Standard MIME type for URI drags; URI lists may contain several URLs
		// with '#' comment lines — take the first non-comment.
		const uriList = dt.getData('text/uri-list');
		if (uriList) {
			for (const line of uriList.split(/\r?\n/)) {
				const trimmed = line.trim();
				if (trimmed && !trimmed.startsWith('#')) return trimmed;
			}
		}
		// Firefox-specific: "URL\ntitle" on two lines.
		const mozUrl = dt.getData('text/x-moz-url');
		if (mozUrl) {
			const firstLine = mozUrl.split(/\r?\n/)[0]?.trim();
			if (firstLine) return firstLine;
		}
		// Fallback: plain text, accepted only when it looks like an http(s)
		// URL, so a drag of selected text is ignored instead of POSTed.
		const plain = dt.getData('text/plain').trim();
		if (/^https?:\/\//i.test(plain)) return plain;
		return null;
	}
}

function isEditableTarget(target: EventTarget | null): boolean {
	return (
		target instanceof HTMLInputElement ||
		target instanceof HTMLTextAreaElement ||
		(target instanceof HTMLElement && target.isContentEditable)
	);
}
