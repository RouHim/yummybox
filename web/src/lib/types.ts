export interface Meal {
	id: number;
	name: string;
	ingredients: string;
	created_at: string; // ISO 8601
	updated_at: string; // ISO 8601
}

export interface MealPayload {
	name: string;
	ingredients: string;
}
