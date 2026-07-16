package com.example.util;

import java.util.List;
import java.util.stream.Collectors;
import com.example.model.Config;

public final class TextUtil {
    public static List<String> trimAll(List<String> in) {
        return in.stream().map(String::trim).collect(Collectors.toList());
    }

    public static String join(List<String> in) {
        return String.join(Config.SEPARATOR, in);
    }

    private TextUtil() {}
}
