import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("offline", Path(__file__).with_name("check-offline.py"))
offline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(offline)


class OfflineTests(unittest.TestCase):
    def test_only_loopback_and_unspecified_are_local(self):
        for value in ("127.0.0.1", "127.4.5.6", "::1", "0.0.0.0", "::", "::ffff:127.0.0.1"):
            self.assertTrue(offline.local(value), value)
        for value in ("fe80::1%en0", "192.168.1.2", "10.0.0.2", "1.1.1.1", "::ffff:192.168.1.2"):
            self.assertFalse(offline.local(value), value)

    def test_process_tree_and_only_new_xpc_helpers(self):
        rows = offline.parse_processes("1 0 launchd\n10 1 ibis-desktop\n11 10 WebKitWebProcess\n12 1 com.apple.WebKit.Networking\n13 1 com.apple.WebKit.Networking\n14 1 Other\n")
        ids, xpc = offline.selected(rows, 10, {1, 12, 14}, "Darwin")
        self.assertEqual(ids, {10, 11, 13})
        self.assertEqual(xpc, {13})
        self.assertEqual(offline.selected(rows, 10, {1, 12, 14}, "Linux"), ({10, 11}, set()))
        with self.assertRaisesRegex(RuntimeError, "exited"):
            offline.selected(rows, 99, set(), "Linux")

    def test_ss_ipv4_ipv6_and_unrelated_process(self):
        rows = 'tcp ESTAB 0 0 10.0.0.2:12 1.1.1.1:443 users:(("app",pid=10,fd=2))\n'
        rows += 'tcp ESTAB 0 0 [::1]:12 [::1]:45 users:(("app",pid=10,fd=3))\n'
        rows += 'udp ESTAB 0 0 [::]:12 [fe80::2%eth0]:53 users:(("app",pid=11,fd=4))\n'
        rows += 'tcp ESTAB 0 0 10.0.0.2:12 8.8.8.8:443 users:(("other",pid=20,fd=2))\n'
        self.assertEqual(offline.parse_ss(rows, {10, 11}), {(10, "tcp", "1.1.1.1:443"), (11, "udp", "[fe80::2%eth0]:53")})

    def test_lsof_field_parser_counts_connected_endpoints(self):
        rows = "p10\nf3\ntIPv4\nPTCP\nn127.0.0.1:12->1.1.1.1:443\nTST=ESTABLISHED\nf4\ntIPv6\nPUDP\nn[::]:12->[fe80::2%en0]:53\np20\nf5\ntIPv4\nPTCP\nn*:123\n"
        self.assertEqual(offline.parse_lsof(rows, {10}), {(10, "tcp", "1.1.1.1:443"), (10, "udp", "[fe80::2%en0]:53")})

    def test_lsof_rejects_malformed_binding_names_and_incomplete_descriptors(self):
        cases = (
            "p10\nf3\nPTCP\nnnot-a-numeric-endpoint\n",
            "p10\nf3\nPTCP\nn127.0.0.1:https\n",
            "p10\nf3\nPTCP\n",
            "p10\nf3\nPTCP\nf4\nPTCP\nn*:123\n",
            "p10\nf3\nPTCP\np11\nf4\nPTCP\nn*:123\n",
        )
        for text in cases:
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    offline.parse_lsof(text, {10, 11})

    def test_lsof_accepts_complete_numeric_bindings_and_known_empty_output(self):
        self.assertEqual(offline.parse_lsof("p10\nf3\ntIPv4\nPTCP\nn*:123\nTST=LISTEN\nf4\ntIPv6\nPUDP\nn[::1]:456\n", {10}), set())
        self.assertEqual(offline.parse_lsof("", {10}), set())

    def test_unrelated_unattributed_socket_is_not_blamed_on_ibis(self):
        self.assertEqual(offline.parse_ss("tcp ESTAB 0 0 10.0.0.1:12 1.1.1.1:443\n", {10}), set())

    def test_malformed_rows_fail_closed(self):
        for parser, text in ((offline.parse_ss, "unexpected"), (offline.parse_lsof, "n1.1.1.1:4->1.1.1.1:5")):
            with self.assertRaises(ValueError):
                parser(text, {10})

    def test_tool_warnings_and_errors_are_not_a_clean_result(self):
        from subprocess import CompletedProcess
        for code, stderr in ((1, ""), (0, "permission denied")):
            with patch.object(offline.subprocess, "run", return_value=CompletedProcess([], code, "", stderr)):
                with self.assertRaisesRegex(RuntimeError, "completely"):
                    offline.command(["ss"])

    def test_time_wait_has_no_live_owner_and_is_not_attributed_to_ibis(self):
        self.assertEqual(offline.parse_ss("tcp TIME-WAIT 0 0 10.0.0.1:12 1.1.1.1:443\n", {10}), set())

    def test_selected_process_fd_permission_denial_fails_closed(self):
        with patch.object(offline.Path, "iterdir", side_effect=PermissionError("denied")):
            with self.assertRaises(PermissionError):
                offline.sample("Linux", {10})


if __name__ == "__main__":
    unittest.main()
