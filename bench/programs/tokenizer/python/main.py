from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "_shared" / "python"))
from runner import main
from kernels import tokenizer

if __name__ == "__main__":
    main(tokenizer, 100000)
