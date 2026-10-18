from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "_shared" / "python"))
from runner import main
from kernels import tail_clique

if __name__ == "__main__":
    main(tail_clique, 10000000)
