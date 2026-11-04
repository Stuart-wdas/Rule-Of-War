"""Exercise real F10/Escape key events and capture hidden/restored native UI."""
import json
from pathlib import Path
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
qa.OUT = ROOT / '.data/render-checks/hide-ui'
CASES = {
    'hidden': ['--hide-ui'],
    'restored': ['--hide-ui', '--restore-ui'],
    'menu': ['--hide-ui', '--hidden-ui-menu'],
}
if __name__ == '__main__':
    results = {}
    for name, flags in CASES.items():
        result = qa.run('ui-' + name, qa.CURRENT,
                        ['--tank-model', '--graphics', 'balanced', *flags])
        report = result['report']
        assert report['hudHidden'] == (name != 'restored')
        assert report['mainMenuVisible'] == (name == 'menu')
        if name == 'restored':
            assert report['unitSpecificBadges'] > 0
            assert report['visibleTacticalUiRoots'] > 0
        else:
            assert report['unitSpecificBadges'] == 0
            assert report['visibleTacticalUiRoots'] == 0
        results[name] = result
        (qa.OUT / 'verification.json').write_text(json.dumps(results, indent=2))
        print('VERIFIED F10', name, flush=True)
