// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import SwiftUI

public struct ContentView: View {
    @AppStorage("setting") var setting = true

    public init() {
    }

    public var body: some View {
        TabView {
            VStack {
                Text("Welcome to the App Fair!")
                Image(systemName: "heart.fill")
                    .foregroundColor(.red)
            }
            .font(.largeTitle)
            .tabItem { Label("App Fair", systemImage: "house.fill") }

            NavigationStack {
                List {
                    ForEach(1..<1_000) { i in
                        NavigationLink("App \(i)", value: i)
                    }
                }
                .navigationTitle("Navigation")
                .navigationDestination(for: Int.self) { i in
                    Text("App \(i)")
                        .font(.title)
                        .navigationTitle("Navigation \(i)")
                }
            }
            .tabItem { Label("Apps", systemImage: "list.bullet") }

            Form {
                Text("Settings")
                    .font(.largeTitle)
                Toggle("Option", isOn: $setting)
            }
            .tabItem { Label("Settings", systemImage: "gearshape.fill") }
        }
    }
}
