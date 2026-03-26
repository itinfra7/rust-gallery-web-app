export function initSearchHelp() {
    const icon = document.getElementById('searchHelpIcon');
    const tooltip = document.getElementById('searchTooltip');

    if (!icon || !tooltip) return;

    icon.addEventListener('mouseenter', () => {
        tooltip.classList.add('show');
    });

    icon.addEventListener('mouseleave', () => {
        tooltip.classList.remove('show');
    });

    icon.addEventListener('click', (e) => {
        e.stopPropagation();
        tooltip.classList.toggle('show');
    });

    document.addEventListener('click', (e) => {
        if (!tooltip.contains(e.target) && e.target !== icon) {
            tooltip.classList.remove('show');
        }
    });
}
