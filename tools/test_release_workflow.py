"""Static checks for the release workflow's verify + signing gates.

The workflow is read as text on purpose: these tests assert observable CI
behaviour (job graph, checksum verification, cosign invocation) without
depending on a YAML parser or executing the release.
"""

from __future__ import annotations
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"


class ReleaseWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = WORKFLOW.read_text(encoding="utf-8")

    def job_block(self, name: str) -> str:
        match = re.search(
            rf"^  {re.escape(name)}:\s*\n(?P<body>(?:^(?:    .*)?\n?)*)",
            self.text,
            re.MULTILINE,
        )
        if match is None:
            self.fail(f"missing job {name!r} in {WORKFLOW}")
        return match.group("body")

    def job_order(self) -> list[str]:
        return re.findall(r"^  ([a-z][a-z0-9_-]*):\s*$", self.text, re.MULTILINE)

    def test_job_order_verify_between_sbom_and_publish(self) -> None:
        order = self.job_order()
        self.assertIn("verify", order, f"job order: {order}")
        self.assertLess(order.index("sbom"), order.index("verify"))
        self.assertLess(order.index("verify"), order.index("publish"))

    def test_publish_depends_on_verify(self) -> None:
        publish = self.job_block("publish")
        match = re.search(r"^    needs:\s*\[([^\]]+)\]\s*$", publish, re.MULTILINE)
        self.assertIsNotNone(match, "publish.needs must be an explicit list")
        needs = {item.strip() for item in match.group(1).split(",")}
        self.assertTrue({"build", "sbom", "verify"}.issubset(needs), f"publish.needs={needs}")

    def test_verify_checks_checksums_on_all_platforms(self) -> None:
        verify = self.job_block("verify")
        self.assertIn("actions/download-artifact@", verify)
        self.assertIn("pattern: release-*", verify)
        self.assertIn("shasum -a 256 -c", verify)
        self.assertIn("Get-FileHash -Algorithm SHA256", verify)

    def test_verify_signs_blobs_with_cosign_keyless(self) -> None:
        verify = self.job_block("verify")
        self.assertIn("id-token: write", verify)
        installer = re.search(r"sigstore/cosign-installer@([0-9a-f]{40})", verify)
        self.assertIsNotNone(installer, "cosign-installer must be pinned by full SHA")
        self.assertRegex(verify, r"cosign sign-blob --yes --bundle\s+\S+\.bundle")
        self.assertRegex(verify, r"--output-signature\s+\S+\.sig")
        self.assertIn("actions/upload-artifact@", verify)
        self.assertIn("name: verify-signatures", verify)

    def test_publish_uploads_signatures(self) -> None:
        publish = self.job_block("publish")
        self.assertIn("name: verify-signatures", publish)
        self.assertIn("*.sig", publish)
        self.assertIn("*.bundle", publish)


if __name__ == "__main__":
    unittest.main()
