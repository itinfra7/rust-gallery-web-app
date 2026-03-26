import { addActive, closeAllLists, createList, positionList } from './autocomplete-dom.js';
import { getSearchTerm, getFinalValue } from './autocomplete-logic.js';

export function setupAutocomplete(inp, list) {
    let currentFocus;

    inp.addEventListener("input", function(e) {
        let val = this.value;
        closeAllLists(null, inp);
        if (!val) return false;

        const { currentTerm, isNegative } = getSearchTerm(val, this.selectionStart);
        if (!currentTerm) return false;

        currentFocus = -1;
        let a = createList(this.id);
        positionList(a, this);

        let count = 0;
        for (let i = 0; i < list.length; i++) {
            if (list[i].toUpperCase().indexOf(currentTerm.toUpperCase()) > -1) {
                let b = document.createElement("DIV");
                b.className = "autocomplete-item";
                
                const index = list[i].toUpperCase().indexOf(currentTerm.toUpperCase());
                let innerHTML = list[i].substring(0, index);
                innerHTML += "<strong>" + list[i].substring(index, index + currentTerm.length) + "</strong>";
                innerHTML += list[i].substring(index + currentTerm.length);
                
                b.innerHTML = innerHTML;
                b.innerHTML += "<input type='hidden' value='" + list[i] + "'>";

                b.addEventListener("click", function(e) {
                    const selected = this.getElementsByTagName("input")[0].value;
                    inp.value = getFinalValue(inp.value, inp.selectionStart, selected, isNegative);
                    closeAllLists(null, inp);
                    inp.focus();
                    if (inp.id === 'searchInput') inp.dispatchEvent(new Event('input'));
                });
                a.appendChild(b);
                count++;
                if (count >= 10) break;
            }
        }
    });

    inp.addEventListener("keydown", function(e) {
        let x = document.getElementById(this.id + "autocomplete-list");
        if (x) x = x.getElementsByTagName("div");
        if (e.keyCode == 40) {
            currentFocus++;
            currentFocus = addActive(x, currentFocus);
        } else if (e.keyCode == 38) {
            currentFocus--;
            currentFocus = addActive(x, currentFocus);
        } else if (e.keyCode == 13) {
            if (currentFocus > -1 && x) {
                x[currentFocus].click();
                e.preventDefault();
            }
        }
    });

    document.addEventListener("click", function (e) {
        closeAllLists(e.target, inp);
    });

    window.addEventListener("resize", function() {
        let x = document.getElementById(inp.id + "autocomplete-list");
        if(x) positionList(x, inp);
    });
}
