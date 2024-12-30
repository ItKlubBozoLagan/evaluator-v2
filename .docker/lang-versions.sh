set -ueo pipefail

echo "---- Python version: ----"
echo "Python verison: "
/usr/bin/python3 --version
echo "-------------------------"
echo

echo "----  GCC version:   ----"
/usr/bin/gcc --version
echo "-------------------------"
echo

echo "----  G++ version:   ----"
/usr/bin/g++ --version
echo "-------------------------"
echo

echo "----   Go version:   ----"
/usr/bin/go version
echo "-------------------------"
echo

echo "---- Rustc version:  ----"
/usr/bin/rustc --version
echo "-------------------------"
echo

echo "----  Java version:  ----"
/usr/bin/java --version
echo "-------------------------"
echo
