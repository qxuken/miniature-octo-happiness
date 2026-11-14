"""Complete finite matrix and negative evidence controls; no native claim."""
import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('resilience', Path(__file__).with_name('resilience.py'))
r = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(r)
SOURCE = 'a' * 40


def state(phase, revision=212, changed=False, size=66154496, detail=None):
    return {'phase': phase, 'revision': revision, 'changed': changed, 'seq': 4, 'roots': 64,
            'digests': r.C.digests(4, changed), 'live_bytes': r.C.CORE // 2, 'peak_bytes': r.C.CORE,
            'cap_bytes': r.C.CORE, 'rss_kib': 1, 'file_bytes': size, 'elapsed_ns': 0,
            'detail': detail if detail is not None else {'fresh_process': True}}


def rows(profile):
    result = []
    if profile.startswith('maintenance-'):
        publish = profile == 'maintenance-publish'; count = 16 if publish else 36
        result.append(state('control', size=63074304))
        cuts = [(s, 0) for s in (range(9, 17) if publish else range(9))]
        if not publish: cuts += [(3, 1), (7, 1), (7, 2)] + [(s, n) for s in [1, 5, 6] for n in [5, 9]]
        for step, n in cuts:
            for after in [False, True]:
                name = f's{step}_n{n}_' + ('after' if after else 'before')
                exchanged = step > 11 or (step == 11 and after)
                result += [{'boundary': name, 'step': step, 'occurrence': n, 'count': 2 if step == 3 else 3 if step == 7 else 10, 'after': after, 'exchanged': exchanged, 'signal': 9, 'orphans': 0},
                           state('killed_' + name, size=63074304 if exchanged else 66154496), state('retried_' + name, size=63074304)]
    else:
        initial = int(profile.split('-')[1]); count = 18
        for second in range(9):
            for persist in [False, True]:
                survives = initial >= 3 or (initial == 2 and (persist or 3 <= second <= 7))
                revision = 212 + int(survives); name = f'i{initial}_f{second}_' + ('persist' if persist else 'drop')
                detail = {'initial': initial, 'second': second, 'persist': persist, 'recovery_error': 'Limit' if second == 8 else 'Io', 'repeat_error': 'Io', 'acknowledged_floor': 213 if initial == 5 else 212}
                result += [state('recovered_' + name, revision, survives, detail=detail), state('retried_' + name, revision + 1, True)]
    return result + [{'cleanup': True, 'cases': count, 'source_unchanged': True}, r.completion(profile, SOURCE)]


class ResilienceControls(unittest.TestCase):
    def test_complete_matrix_and_corruption_omission_type_identity_rejections(self):
        for profile in r.PROFILES:
            good = rows(profile); r.check_rows(good, profile, SOURCE)
            bads = [[], good[:-1], good[1:], good + [good[-1]], good[::-1]]
            for field, value in [('source_id', 'wrong'), ('features', 4.0), ('complete', 1)]:
                bad = copy.deepcopy(good); bad[-1][field] = value; bads.append(bad)
            bad = copy.deepcopy(good); bad[-2]['source_unchanged'] = False; bads.append(bad)
            bad = copy.deepcopy(good); bad[-2]['cases'] = 18.0; bads.append(bad)
            index = 2 if profile.startswith('maintenance-') else 0
            for field, value in [('revision', 1), ('roots', 63), ('roots', 64.0), ('seq', 4.0), ('digests', [0]*4), ('peak_bytes', r.C.CORE+1)]:
                bad = copy.deepcopy(good); bad[index][field] = value; bads.append(bad)
            if profile.startswith('maintenance-'):
                for field, value in [('signal', 15), ('exchanged', not good[1]['exchanged']), ('occurrence', 100), ('orphans', 2), ('after', 0)]:
                    bad = copy.deepcopy(good); bad[1][field] = value; bads.append(bad)
                bad = copy.deepcopy(good); bad[4]['count'] += 1; bads.append(bad)
                bad = copy.deepcopy(good); bad[2]['detail']['fresh_process'] = 1; bads.append(bad)
            else:
                for field, value in [('recovery_error', 'Busy'), ('acknowledged_floor', 999), ('initial', -1), ('persist', 0)]:
                    bad = copy.deepcopy(good); bad[0]['detail'][field] = value; bads.append(bad)
            for bad in bads:
                with self.subTest(profile=profile), self.assertRaises(AssertionError): r.check_rows(bad, profile, SOURCE)

    def test_case_control_and_namespace_order_are_not_interchangeable(self):
        good = rows('recovery-2')
        # Volatile root is dropped before stabilization, but persists if either
        # the first recovery barrier or the external crash persists pending data.
        self.assertEqual([x['revision'] for x in good[:-2:2]], [212,213,212,213,212,213,213,213,213,213,213,213,213,213,213,213,212,213])
        good = rows('recovery-5')
        self.assertEqual({x['revision'] for x in good[:-2:2]}, {213})
        good = rows('maintenance-publish')
        self.assertFalse(good[13]['exchanged'])  # before Exchange (step 11)
        self.assertTrue(good[16]['exchanged'])   # after Exchange


if __name__ == '__main__': unittest.main()
