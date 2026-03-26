export function addActive(x, currentFocus) {
    if (!x) return false;
    removeActive(x);
    if (currentFocus >= x.length) currentFocus = 0;
    if (currentFocus < 0) currentFocus = (x.length - 1);
    x[currentFocus].classList.add("autocomplete-active");
    return currentFocus;
}

export function removeActive(x) {
    for (let i = 0; i < x.length; i++) {
        x[i].classList.remove("autocomplete-active");
    }
}

export function closeAllLists(elmnt, inp) {
    let x = document.getElementsByClassName("autocomplete-items");
    for (let i = 0; i < x.length; i++) {
        if (elmnt != x[i] && elmnt != inp) {
            x[i].parentNode.removeChild(x[i]);
        }
    }
}

export function createList(id) {
    let a = document.createElement("DIV");
    a.setAttribute("id", id + "autocomplete-list");
    a.setAttribute("class", "autocomplete-items");
    document.body.appendChild(a);
    return a;
}

export function positionList(list, input) {
    const rect = input.getBoundingClientRect();
    list.style.left = (rect.left + window.scrollX) + "px";
    list.style.top = (rect.bottom + window.scrollY) + "px";
    list.style.width = rect.width + "px";
}
