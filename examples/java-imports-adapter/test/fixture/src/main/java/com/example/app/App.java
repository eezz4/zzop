package com.example.app;

import java.util.List;
import com.example.model.*;
import com.example.util.TextUtil;
import static com.example.util.TextUtil.trimAll;

public class App {
    public static void main(String[] args) {
        List<String> lines = trimAll(List.of(" a ", " b "));
        System.out.println(TextUtil.join(lines));
    }
}
