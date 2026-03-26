export function getSearchTerm(val, cursorPosition) {
    const textBefore = val.substring(0, cursorPosition);
    const lastSep = Math.max(textBefore.lastIndexOf(','), textBefore.lastIndexOf(' '));
    const startIndex = lastSep + 1;
    let rawTerm = textBefore.substring(startIndex).trim();
    let isNegative = false;

    if (rawTerm.startsWith('-')) {
        isNegative = true;
        rawTerm = rawTerm.substring(1);
    }

    return { currentTerm: rawTerm, startIndex, isNegative };
}

export function getFinalValue(val, cursorPosition, selectedValue, isNegative) {
    const textBefore = val.substring(0, cursorPosition);
    const textAfter = val.substring(cursorPosition);
    
    const lastComma = textBefore.lastIndexOf(',');
    const lastSpace = textBefore.lastIndexOf(' ');
    const lastSep = Math.max(lastComma, lastSpace);

    const prefix = textBefore.substring(0, lastSep + 1);
    
    let finalSelected = selectedValue;
    if (isNegative) {
        finalSelected = "-" + selectedValue;
    }

    return prefix + finalSelected + " " + textAfter;
}
