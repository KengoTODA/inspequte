import java.util.Comparator;
import java.util.List;

public final class ClassA {
    public sealed interface ClassB permits ClassC, ClassD {
        String methodX();
    }

    public record ClassC(String varOne, int varTwo) implements ClassB {
        @Override
        public String methodX() {
            return varOne + varTwo;
        }
    }

    public static final class ClassD implements ClassB {
        private final String varOne;

        public ClassD(String varOne) {
            this.varOne = varOne;
        }

        @Override
        public String methodX() {
            return varOne;
        }
    }

    public String methodY(List<String> varOne) {
        var varTwo = varOne.stream()
                .map(String::strip)
                .sorted(Comparator.naturalOrder())
                .toList();
        return String.join(",", varTwo);
    }
}
