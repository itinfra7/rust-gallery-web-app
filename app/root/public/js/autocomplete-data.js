import { state } from './state.js';

export async function fetchSuggestions() {
    try {
        const [tagsRes, searchRes] = await Promise.all([
            fetch('/api/tags'),
            fetch('/api/search/suggestions')
        ]);

        if (tagsRes.ok) {
            state.allTags = await tagsRes.json();
        }
        if (searchRes.ok) {
            state.searchSuggestions = await searchRes.json();
        }
    } catch (e) {
        console.error("Failed to fetch suggestions");
    }
}
