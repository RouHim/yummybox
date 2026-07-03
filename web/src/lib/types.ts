export interface IngredientQuantity {
	name: string;
	quantity: string | null;
}

export interface Meal {
	id: number;
	name: string;
	ingredients: IngredientQuantity[];
	instructions: string;
	last_planned_at: string | null; // ISO 8601 or null
	created_at: string; // ISO 8601
	updated_at: string; // ISO 8601
	has_image: boolean;
}

export interface NewIngredientLine {
	name: string;
	quantity: string | null;
}

export interface MealPayload {
	name: string;
	ingredients: NewIngredientLine[];
	instructions: string;
}

export interface NumericTotal {
	value: number;
	unit: string | null;
}

export interface IngredientSummaryEntry {
	name: string;
	numeric_total: NumericTotal | null;
	non_numeric: string[];
}

export interface Plan {
	id: number;
	year: number;
	week_number: number;
	created_at: string;
	meals: Meal[];
	ingredient_summary: IngredientSummaryEntry[];
}

export interface PlanSummaryItem {
	year: number;
	week_number: number;
	id: number;
	meal_count: number;
}

export interface NewPlanRequest {
	year: number;
	week_number: number;
	meal_count: number;
}

export interface PlanPatch {
	meal_ids: number[];
}

export interface ImportDraft {
	name: string;
	ingredients: NewIngredientLine[];
	instructions: string;
	imageBase64: string | null;
}

export interface ImportFromUrlRequest {
	url: string;
}

export interface ImportFromPasteRequest {
	content: string;
}

export interface BulkImportRequest {
	urls: string[];
}

export interface BulkImportFailure {
	url: string;
	reason: string;
}

export interface BulkImportResult {
	created: Meal[];
	failed: BulkImportFailure[];
}

export interface ZipImportFailure {
	source: string;
	reason: string;
}

export interface ZipImportResult {
	created: Meal[];
	skipped: number;
	failed: ZipImportFailure[];
}


export interface LlmProviderInfo {
    id: string;
    name: string;
    envVar: string;
    configured: boolean;
    supportsCustomEndpoint: boolean;
}

export interface LlmProvidersResponse {
    providers: LlmProviderInfo[];
}

export interface LlmModelsResponse {
    models: string[];
}
