"""Validate fail-closed benchmark cardinalities and staged policy separation."""
import importlib.util
from pathlib import Path
import unittest
script=Path(__file__).resolve().parents[1]/'benchmark-operation-paths.py'
spec=importlib.util.spec_from_file_location('benchmark_operation_paths',script)
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
class TransactionBenchTests(unittest.TestCase):
    def test_schema_missing_rows_fails(self):
        with self.assertRaises(ValueError):module.strict_csv('sectors,shape\n1,a',module.NUMERIC_TRANSACTION|{'shape'},'transaction')
    def test_disables_batch_in_raw_emulated_or_gapped(self):
        rows=[]
        for n in (128,1024,4096,16384):
            for shape in ('contiguous','strided'):
                for maximum in (1,128):
                    single=shape=='strided' or maximum==1
                    rows.append(dict(sectors=n,shape=shape,io_limit=maximum,syncs=2,
                        single_reads=2*(n+1) if single else 2,
                        batch_reads=0 if single else 2,
                        single_writes=n+1 if single else 1,
                        batch_writes=0 if single else 1))
        module.validate_transaction(rows)
        rows[0]['single_reads']=0
        with self.assertRaises(AssertionError):module.validate_transaction(rows)
        rows[0]['single_reads']=2*(128+1)
        rows[0]['batch_writes']=1
        with self.assertRaises(AssertionError):module.validate_transaction(rows)
        with self.assertRaises(ValueError):module.validate_transaction(rows[:-1])
if __name__=='__main__':unittest.main()
