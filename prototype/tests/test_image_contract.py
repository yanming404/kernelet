#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Mutate a real full-kernel image and exercise the production offline checker."""
import contextlib
import importlib.util
import io
import pathlib
import struct
import sys
import tempfile
import unittest

IMAGE = pathlib.Path(sys.argv.pop(1)).read_bytes()
SPEC = importlib.util.spec_from_file_location('image_checker', pathlib.Path(__file__).resolve().parents[1] / 'tools/check-image.py')
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class ImageContract(unittest.TestCase):
    def setUp(self):
        self.data = bytearray(IMAGE)
        self.entry = struct.unpack_from('<Q', self.data, 24)[0]
        self.phoff = struct.unpack_from('<Q', self.data, 32)[0]
        self.phnum = struct.unpack_from('<H', self.data, 56)[0]
        self.headers = [(self.phoff + 56*i, struct.unpack_from('<IIQQQQQQ', self.data, self.phoff + 56*i)) for i in range(self.phnum)]
        self.loads = [h for _, h in self.headers if h[0] == 1]
        dynamic = next(h for _, h in self.headers if h[0] == 2)
        self.tags = {struct.unpack_from('<q', self.data, p)[0]: p for p in range(dynamic[2], dynamic[2]+dynamic[5], 16)}
        rela_va = struct.unpack_from('<Q', self.data, self.tags[7]+8)[0]
        self.rela = self.file_offset(rela_va)
        self.rela_size = struct.unpack_from('<Q', self.data, self.tags[8]+8)[0]

    def file_offset(self, va):
        h = next(h for h in self.loads if h[3] <= va < h[3]+h[5])
        return h[2]+va-h[3]

    def check(self, error=None):
        with tempfile.TemporaryDirectory(prefix='klet-image-contract-') as root:
            image = pathlib.Path(root) / 'mutated.image'
            image.write_bytes(self.data)
            with contextlib.redirect_stdout(io.StringIO()):
                if error:
                    with self.assertRaisesRegex(ValueError, error):
                        CHECKER.check(image)
                else:
                    CHECKER.check(image)

    def test_real_full_kernel_is_accepted(self):
        self.check()

    def test_old_abi_is_rejected(self):
        struct.pack_into('<Q', self.data, self.file_offset(self.entry), 7)
        self.check('ABI version')

    def test_shared_relocation_target_is_rejected(self):
        struct.pack_into('<Q', self.data, self.rela, 8)
        self.check('allowed load segment')

    def test_external_symbol_relocation_is_rejected(self):
        struct.pack_into('<Q', self.data, self.rela+8, (1 << 32) | 8)
        self.check('symbol-free')

    def test_duplicate_relocation_is_rejected(self):
        target = struct.unpack_from('<Q', self.data, self.rela)[0]
        struct.pack_into('<Q', self.data, self.rela+24, target)
        self.check('duplicate')

    def test_unaligned_relocation_is_rejected(self):
        target = struct.unpack_from('<Q', self.data, self.rela)[0]
        struct.pack_into('<Q', self.data, self.rela, target+1)
        self.check('unaligned')

    def test_entry_callback_must_be_executable(self):
        for p in range(self.rela, self.rela+self.rela_size, 24):
            if struct.unpack_from('<Q', self.data, p)[0] == self.entry+8:
                struct.pack_into('<q', self.data, p+16, self.loads[1][3])
                break
        else:
            self.fail('real entry relocation missing')
        self.check('entry callback')

    def test_image_span_limit_is_enforced(self):
        p, _ = [h for h in self.headers if h[1][0] == 1][-1]
        struct.pack_into('<Q', self.data, p+40, 16*1024*1024)
        self.check('16 MiB')

    def test_automatic_constructor_tag_is_rejected(self):
        struct.pack_into('<q', self.data, self.tags[21], 12)  # DT_DEBUG -> DT_INIT
        self.check('unsupported dynamic tags')

    def test_interpreter_is_rejected(self):
        struct.pack_into('<I', self.data, self.headers[0][0], 3)
        self.check('unsupported program header')


if __name__ == '__main__':
    unittest.main()
